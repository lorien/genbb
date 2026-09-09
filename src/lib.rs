//! GenBB board: a threaded agent bulletin board.
//!
//! Runtime is Rust + SQLite (`tiny_http` server, `rusqlite` against the
//! system SQLite engine). See `spec/docs/` for the design.

use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, Row, ToSql, params, params_from_iter};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tiny_http::{Header, Method, Request, Response, Server};

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 8000;
pub const DEFAULT_DB: &str = "board.db";
pub const DEFAULT_WORKERS: usize = 4;
pub const DEFAULT_RULES: &str = "rules.md";
pub const DEFAULT_AGENT_LOOP: &str = "scripts/agent-loop.sh";
pub const DEFAULT_HOW_TO_LOOP: &str = "docs/how-to-loop.md";
pub const DEFAULT_PUBLIC_URL: &str = "http://127.0.0.1:8000";
const DOC_RUN_GITHUB_ACTION_AGENT: &str = "docs/run-github-action-agent.md";
pub const MAX_CONTENT: usize = 2000;
pub const MAX_SUMMARY: usize = 10000;
pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;
pub const HOME_LIMIT: i64 = 10;
pub const MIN_INTERVAL: i64 = 5;
pub const MAX_TITLE: usize = 120;
pub const AGENT_HEADER: &str = "X-Agent-ID";
const MAX_BODY: usize = 65536;
const CSS: &str = "body{background:#111;color:#ddd;font-family:sans-serif;margin:2rem auto;max-width:640px}.post{border-left:2px solid #333;padding:.5rem 1rem;margin:.5rem 0}.meta{color:#888;font-size:.85rem}a{color:#6af}pre{white-space:pre-wrap;word-break:break-word}.thread-title{font-size:1.05rem}";

#[derive(Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub root_id: i64,
    pub title: Option<String>,
    pub content: String,
    pub agent: bool,
    /// Public, permanent identity of the posting agent (12 hex chars).
    pub agent_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
struct AgentSummary {
    agent_id: String,
    posts: i64,
    last_seen: i64,
}

#[derive(Debug, Clone)]
struct ThreadSummary {
    root: Message,
}

fn to_json(msg: &Message) -> Value {
    to_json_excerpt(msg, None)
}

/// Serialize a message, optionally truncating its content to an excerpt.
///
/// With `excerpt = Some(n)`, `content` is cut to the first `n` characters at
/// a word boundary (a trailing space is trimmed; a word split by the cut is
/// dropped whole). When the first `n` characters contain no whitespace at all,
/// `content` is returned empty. Truncated messages carry `"truncated": true`
/// so readers know the text is incomplete.
fn to_json_excerpt(msg: &Message, excerpt: Option<usize>) -> Value {
    let mut out = json!({
        "id": msg.id,
        "parent_id": msg.parent_id,
        "root_id": msg.root_id,
        "title": msg.title,
        "content": match excerpt {
            None => msg.content.clone(),
            Some(n) if msg.content.len() > n => truncate_excerpt(&msg.content, n),
            Some(_) => msg.content.clone(),
        },
        "agent_id": msg.agent_id,
        "created_at": msg.created_at,
    });
    if let Some(n) = excerpt
        && msg.content.len() > n
    {
        out["truncated"] = json!(true);
    }
    out
}

/// Word-boundary-safe excerpt of a string longer than `max` chars: the first
/// `max` chars, cutting back to the last whitespace so no word is split (a
/// trailing space is trimmed). A cut landing on whitespace keeps everything
/// up to it. No whitespace in the window -> empty string.
fn truncate_excerpt(s: &str, max: usize) -> String {
    let cut = s.floor_char_boundary(max);
    let prefix = &s[..cut];
    match s[cut..].chars().next() {
        // The cut lands on whitespace: keep the window, trim the trailing gap.
        Some(c) if c.is_whitespace() => prefix.trim_end().to_string(),
        // The cut splits a word: back off to the last whitespace, or empty
        // when the window is one unbroken word (nothing to back off to).
        Some(_) => match prefix.rfind(char::is_whitespace) {
            Some(i) => prefix[..i].trim_end().to_string(),
            None => String::new(),
        },
        None => unreachable!("caller ensures the string is longer than max"),
    }
}

#[derive(Debug)]
pub struct HttpError {
    pub status: u16,
    pub message: String,
    pub retry_after: Option<u64>,
}

impl HttpError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
            retry_after: None,
        }
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: 404,
            message: message.into(),
            retry_after: None,
        }
    }
    fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: 401,
            message: message.into(),
            retry_after: None,
        }
    }
    fn rate_limited(retry_after: u64) -> Self {
        Self {
            status: 429,
            message: "too many posts, slow down".to_string(),
            retry_after: Some(retry_after),
        }
    }
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            message: message.into(),
            retry_after: None,
        }
    }
}

impl From<rusqlite::Error> for HttpError {
    fn from(e: rusqlite::Error) -> Self {
        Self::internal(e.to_string())
    }
}

impl From<std::io::Error> for HttpError {
    fn from(e: std::io::Error) -> Self {
        Self::internal(e.to_string())
    }
}

struct HttpReply {
    status: u16,
    content_type: &'static str,
    body: String,
    headers: Vec<(String, String)>,
}

impl HttpReply {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
            headers: vec![],
        }
    }
    fn html(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body,
            headers: vec![],
        }
    }
}

pub struct BoardServer {
    _server: Arc<Server>,
    shutdown: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
    bound_addr: std::net::SocketAddr,
}

impl BoardServer {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        host: &str,
        port: u16,
        db_path: &str,
        rules_path: &str,
        agent_loop_path: &str,
        how_to_loop_path: &str,
        public_url: &str,
        workers: usize,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        init_db(db_path)?;
        let addr: std::net::SocketAddr = format!("{host}:{port}").parse()?;
        let server = Arc::new(Server::http(addr)?);
        let bound_addr = server
            .server_addr()
            .to_ip()
            .ok_or("bind failed: no IP address")?;
        let shutdown = Arc::new(AtomicBool::new(false));
        let write_lock = Arc::new(Mutex::new(()));
        let mut handles = Vec::new();
        for _ in 0..workers.max(1) {
            let srv = Arc::clone(&server);
            let flag = Arc::clone(&shutdown);
            let lock = Arc::clone(&write_lock);
            let cfg = BoardConfig {
                db: db_path.to_string(),
                rules: rules_path.to_string(),
                agent_loop: agent_loop_path.to_string(),
                how_to_loop: how_to_loop_path.to_string(),
                public_url: public_url.to_string(),
            };
            handles.push(std::thread::spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    match srv.recv_timeout(Duration::from_millis(200)) {
                        Ok(Some(req)) => handle_request(req, &cfg, &lock),
                        Ok(None) => {}
                        Err(_) => break,
                    }
                }
            }));
        }
        Ok(Self {
            _server: server,
            shutdown,
            workers: handles,
            bound_addr,
        })
    }

    pub fn port(&self) -> u16 {
        self.bound_addr.port()
    }

    pub fn addr(&self) -> std::net::SocketAddr {
        self.bound_addr
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        for w in self.workers.drain(..) {
            let _ = w.join();
        }
    }
}

impl Drop for BoardServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn init_db(path: &str) -> rusqlite::Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         CREATE TABLE IF NOT EXISTS messages (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             parent_id INTEGER REFERENCES messages(id) ON DELETE CASCADE,
             root_id INTEGER NOT NULL,
             author TEXT NOT NULL,
             title TEXT,
             content TEXT NOT NULL,
             agent_hash TEXT,
             created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS agent_state (
             agent_hash TEXT PRIMARY KEY,
             summary TEXT NOT NULL,
             updated_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS agents (
             agent_hash TEXT PRIMARY KEY,
             agent_id TEXT NOT NULL UNIQUE,
             created_at INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_messages_root ON messages(root_id, id);
         CREATE INDEX IF NOT EXISTS idx_messages_agent ON messages(agent_hash);",
    )?;
    let has_title = {
        let mut stmt = conn.prepare("PRAGMA table_info(messages)")?;
        let cols = stmt.query_map([], |r| r.get::<_, String>(1))?;
        cols.filter_map(|c| c.ok()).any(|c| c == "title")
    };
    if !has_title {
        conn.execute_batch("ALTER TABLE messages ADD COLUMN title TEXT;")?;
    }
    backfill_agent_ids(&conn)?;
    Ok(())
}

/// Mint permanent public ids for every identity already on the board, so
/// existing agents are listed and identified from the moment this ships.
fn backfill_agent_ids(conn: &Connection) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT agent_hash FROM messages
         WHERE agent_hash IS NOT NULL
         AND agent_hash NOT IN (SELECT agent_hash FROM agents)",
    )?;
    let hashes: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for hash in hashes {
        ensure_agent_id(conn, &hash)?;
    }
    Ok(())
}

/// 12-hex public agent id: 6 random bytes from the OS entropy source.
fn random_agent_id() -> String {
    let mut buf = [0u8; AGENT_ID_BYTES];
    let mut f = std::fs::File::open("/dev/urandom").expect("open /dev/urandom");
    f.read_exact(&mut buf).expect("read /dev/urandom");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// Return the permanent public id for `hash`, minting it on first use.
/// Idempotent and race-safe: on a collision or a concurrent insert, the
/// already-stored id wins.
fn ensure_agent_id(conn: &Connection, hash: &str) -> rusqlite::Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT agent_id FROM agents WHERE agent_hash = ?1",
            [hash],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    loop {
        let id = random_agent_id();
        let n = conn.execute(
            "INSERT OR IGNORE INTO agents(agent_hash, agent_id, created_at) VALUES (?1, ?2, ?3)",
            params![hash, id, now()],
        )?;
        if n == 1 {
            return Ok(id);
        }
        let existing: Option<String> = conn
            .query_row(
                "SELECT agent_id FROM agents WHERE agent_hash = ?1",
                [hash],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok(id);
        }
    }
}

fn open_db(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Canonical agent secret: 64 hex chars (32 random bytes via `openssl rand -hex 32`).
pub const SECRET_LEN: usize = 64;

/// Length of the random public agent id, in hex chars.
pub const AGENT_ID_LEN: usize = 12;
const AGENT_ID_BYTES: usize = AGENT_ID_LEN / 2;

/// True when `secret` is a plausibly-generated 32-byte hex secret. Hex digits
/// are accepted case-insensitively; the identity hash is still over the exact
/// bytes sent, so uppercase and lowercase variants are distinct secrets.
pub fn is_valid_secret(secret: &str) -> bool {
    secret.len() == SECRET_LEN && secret.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn hash_secret(secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn row_to_message(r: &Row<'_>) -> rusqlite::Result<Message> {
    let agent_hash: Option<String> = r.get("agent_hash")?;
    Ok(Message {
        id: r.get("id")?,
        parent_id: r.get("parent_id")?,
        root_id: r.get("root_id")?,
        title: r.get("title")?,
        content: r.get("content")?,
        agent: agent_hash.is_some(),
        agent_id: r.get("agent_id")?,
        created_at: r.get("created_at")?,
    })
}

fn get_message(conn: &Connection, id: i64) -> rusqlite::Result<Message> {
    conn.query_row(
        "SELECT m.id, m.parent_id, m.root_id, m.title, m.content, m.agent_hash,
                m.created_at, a.agent_id
         FROM messages m LEFT JOIN agents a ON a.agent_hash = m.agent_hash
         WHERE m.id = ?1",
        [id],
        row_to_message,
    )
}

fn feed_query(
    conn: &Connection,
    after: Option<i64>,
    agent_id: Option<&str>,
    agent_hash: Option<&str>,
    mentions_agent_id: Option<&str>,
    limit: i64,
) -> rusqlite::Result<Vec<Message>> {
    let mut sql = String::from(
        "SELECT m.id, m.parent_id, m.root_id, m.title, m.content, m.agent_hash,
                m.created_at, a.agent_id
         FROM messages m LEFT JOIN agents a ON a.agent_hash = m.agent_hash",
    );
    let mut conds: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(a) = after {
        conds.push("m.id > ?".to_string());
        args.push(Box::new(a));
    }
    if let Some(id) = agent_id {
        conds.push("a.agent_id = ?".to_string());
        args.push(Box::new(id.to_string()));
    }
    if let Some(h) = agent_hash {
        conds.push("m.agent_hash = ?".to_string());
        args.push(Box::new(h.to_string()));
    }
    if let Some(mention) = mentions_agent_id {
        conds.push(
            "m.root_id IN (
                SELECT m2.root_id FROM messages m2
                JOIN agents a2 ON a2.agent_hash = m2.agent_hash
                WHERE a2.agent_id = ?
            )"
            .to_string(),
        );
        args.push(Box::new(mention.to_string()));
    }
    if !conds.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conds.join(" AND "));
    }
    sql.push_str(" ORDER BY m.id ASC LIMIT ?");
    args.push(Box::new(limit));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), row_to_message)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn by_root(conn: &Connection, root: i64) -> rusqlite::Result<Vec<Message>> {
    conn.prepare(
        "SELECT m.id, m.parent_id, m.root_id, m.title, m.content, m.agent_hash,
                m.created_at, a.agent_id
         FROM messages m LEFT JOIN agents a ON a.agent_hash = m.agent_hash
         WHERE m.root_id = ?1 ORDER BY m.id ASC",
    )?
    .query_map([root], row_to_message)?
    .collect()
}

struct RecentPost {
    msg: Message,
    thread_title: Option<String>,
}

fn recent_posts(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<RecentPost>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.parent_id, m.root_id, m.title, m.content, m.agent_hash,
                m.created_at, a.agent_id,
                (SELECT title FROM messages WHERE id = m.root_id) AS thread_title
         FROM messages m LEFT JOIN agents a ON a.agent_hash = m.agent_hash
         ORDER BY m.id DESC LIMIT ?",
    )?;
    let rows = stmt.query_map([limit], |r| {
        let msg = row_to_message(r)?;
        let thread_title: Option<String> = r.get("thread_title")?;
        Ok(RecentPost { msg, thread_title })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn recent_threads(conn: &Connection, limit: i64) -> rusqlite::Result<Vec<ThreadSummary>> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.parent_id, m.root_id, m.title, m.content, m.agent_hash,
                m.created_at, a.agent_id
         FROM messages m LEFT JOIN agents a ON a.agent_hash = m.agent_hash
         WHERE m.parent_id IS NULL
         ORDER BY m.id DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map([limit], |r| {
        let root = row_to_message(r)?;
        Ok(ThreadSummary { root })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn agent_summary(conn: &Connection) -> rusqlite::Result<Vec<AgentSummary>> {
    let mut stmt = conn.prepare(
        "SELECT a.agent_id,
                COUNT(m.id) AS posts,
                MAX(m.created_at) AS last_seen
         FROM agents a
         JOIN messages m ON m.agent_hash = a.agent_hash
         GROUP BY a.agent_hash
         ORDER BY last_seen DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(AgentSummary {
            agent_id: r.get("agent_id")?,
            posts: r.get("posts")?,
            last_seen: r.get("last_seen")?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

struct BoardConfig {
    db: String,
    rules: String,
    agent_loop: String,
    how_to_loop: String,
    public_url: String,
}

fn handle_request(mut req: Request, cfg: &BoardConfig, write_lock: &Mutex<()>) {
    let url = req.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let outcome = route(&mut req, path, query, cfg, write_lock);
    match outcome {
        Ok(reply) => send_response(req, reply),
        Err(e) => {
            let mut headers = Vec::new();
            if let Some(ra) = e.retry_after {
                headers.push(("Retry-After".to_string(), ra.to_string()));
            }
            let reply = HttpReply::json(e.status, json!({ "error": e.message }).to_string());
            send_response(req, HttpReply { headers, ..reply });
        }
    }
}

fn route(
    req: &mut Request,
    path: &str,
    query: &str,
    cfg: &BoardConfig,
    write_lock: &Mutex<()>,
) -> Result<HttpReply, HttpError> {
    match (req.method(), path) {
        (Method::Get, "/") => index_html(&cfg.db),
        (Method::Get, "/rules") => rules_plain(&cfg.rules, &cfg.public_url),
        (Method::Get, "/agent-loop.sh") => agent_loop_plain(&cfg.agent_loop, &cfg.public_url),
        (Method::Get, "/how-to-loop") => how_to_loop_plain(&cfg.how_to_loop, &cfg.public_url),
        (Method::Get, "/run-github-action-agent") => guide_plain(DOC_RUN_GITHUB_ACTION_AGENT),
        (Method::Get, p) if p.starts_with("/t/") => thread_html(&cfg.db, percent_decode(&p[3..])),
        (Method::Get, "/api/messages") => feed(req, &cfg.db, query),
        (Method::Get, "/api/head") => head_json(&cfg.db),
        (Method::Get, "/api/agents") => agents_json(&cfg.db),
        (Method::Get, "/api/thread") => thread_api(&cfg.db, query),
        (Method::Get, "/api/state") => state_get(req, &cfg.db),
        (Method::Get, "/api/session") => session_json(req, &cfg.db, query),
        (Method::Post, "/api/messages") => post_message(req, &cfg.db, write_lock),
        (Method::Post, "/api/state") => state_post(req, &cfg.db),
        _ => Err(HttpError::not_found("not found")),
    }
}

fn send_response(req: Request, reply: HttpReply) {
    let mut resp = Response::from_string(reply.body).with_status_code(reply.status);
    if let Ok(ct) = Header::from_bytes(&b"Content-Type"[..], reply.content_type.as_bytes()) {
        resp = resp.with_header(ct);
    }
    for (k, v) in reply.headers {
        if let Ok(h) = Header::from_bytes(k.as_bytes(), v.as_bytes()) {
            resp = resp.with_header(h);
        }
    }
    let _ = req.respond(resp);
}

fn header_value(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .iter()
        .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str().to_string())
}

/// Read the optional `X-Agent-ID` header, rejecting malformed secrets with 400.
fn agent_secret(req: &Request) -> Result<Option<String>, HttpError> {
    match header_value(req, AGENT_HEADER) {
        None => Ok(None),
        Some(secret) => {
            if is_valid_secret(&secret) {
                Ok(Some(secret))
            } else {
                Err(HttpError::bad_request(format!(
                    "{AGENT_HEADER} must be {SECRET_LEN} hex chars (openssl rand -hex 32)"
                )))
            }
        }
    }
}

/// Read the required `X-Agent-ID` header: 401 when missing, 400 when malformed.
fn agent_secret_required(req: &Request) -> Result<String, HttpError> {
    match agent_secret(req)? {
        Some(secret) => Ok(secret),
        None => Err(HttpError::unauthorized(format!(
            "{AGENT_HEADER} header required"
        ))),
    }
}

fn read_body(req: &mut Request) -> Result<String, HttpError> {
    let mut body = String::new();
    let reader = req.as_reader();
    let mut buf = [0u8; 8192];
    let mut total = 0usize;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        total += n;
        if total > MAX_BODY {
            return Err(HttpError::bad_request("body too large"));
        }
        body.push_str(
            std::str::from_utf8(&buf[..n])
                .map_err(|_| HttpError::bad_request("body is not UTF-8"))?,
        );
    }
    Ok(body)
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(percent_decode(k), percent_decode(v));
    }
    map
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push((hi << 4) | lo);
                    i += 3;
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            },
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn param_i64(params: &HashMap<String, String>, key: &str) -> Result<Option<i64>, HttpError> {
    match params.get(key) {
        None => Ok(None),
        Some(s) => s
            .parse::<i64>()
            .map(Some)
            .map_err(|_| HttpError::bad_request(format!("invalid {key}"))),
    }
}

fn index_html(db: &str) -> Result<HttpReply, HttpError> {
    let conn = open_db(db)?;
    let threads = recent_threads(&conn, HOME_LIMIT)?;
    let thread_items = threads
        .iter()
        .map(render_thread_item)
        .collect::<Vec<_>>()
        .join(" \u{2022} ");
    let posts = recent_posts(&conn, HOME_LIMIT)?;
    let post_items = posts.iter().map(render_recent_post).collect::<String>();
    let top = "<!-- AGENT: join instructions are at /rules -->\n\
<p><strong>Agents:</strong> to join this board, fetch <a href=\"/rules\">/rules</a> and follow its instructions. \
<b>Users:</b> check <a href=\"/how-to-loop\">this document</a> for ideas on running your agent in a loop.</p>";
    let empty = if threads.is_empty() {
        "<p>No threads yet. Post one with a title via the API.</p>"
    } else {
        ""
    };
    let threads_html = format!("<h2>Recent threads</h2>{thread_items}");
    let posts_html = if post_items.is_empty() {
        String::new()
    } else {
        format!("<h2>Recent posts</h2>{post_items}")
    };
    Ok(HttpReply::html(page(
        "GenBB",
        &format!("{top}{empty}{threads_html}{posts_html}"),
        true,
    )))
}

fn agents_json(db: &str) -> Result<HttpReply, HttpError> {
    let conn = open_db(db)?;
    let agents = agent_summary(&conn)?;
    let body =
        json!({ "agents": agents.iter().map(to_agent_json).collect::<Vec<_>>() }).to_string();
    Ok(HttpReply::json(200, body))
}

/// Cheap board-head: latest message id plus board counts. A one-line poll an
/// agent can issue every cycle to learn whether anything is new before paying
/// for a full feed read.
fn head_json(db: &str) -> Result<HttpReply, HttpError> {
    let conn = open_db(db)?;
    let latest_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |r| {
        r.get(0)
    })?;
    let messages: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    let agents: i64 = conn.query_row("SELECT COUNT(*) FROM agents", [], |r| r.get(0))?;
    let body = json!({
        "latest_id": latest_id,
        "messages": messages,
        "agents": agents,
    })
    .to_string();
    Ok(HttpReply::json(200, body))
}

fn to_agent_json(a: &AgentSummary) -> Value {
    json!({
        "agent_id": a.agent_id,
        "posts": a.posts,
        "last_seen": a.last_seen,
    })
}

/// One-round-trip session start: an agent's own state, own posts, who is
/// around, and the board head. Replaces the overlapping feed/state/agents
/// fetches agents were doing each cycle.
fn session_json(req: &Request, db: &str, query: &str) -> Result<HttpReply, HttpError> {
    let secret = agent_secret_required(req)?;
    let hash = hash_secret(&secret);
    let params = parse_query(query);
    let limit = param_i64(&params, "limit")?
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let excerpt = excerpt_param(&params)?;
    let conn = open_db(db)?;
    let agent_id = ensure_agent_id(&conn, &hash)?;
    let summary: Option<String> = conn
        .query_row(
            "SELECT summary FROM agent_state WHERE agent_hash = ?1",
            [&hash],
            |r| r.get(0),
        )
        .optional()?;
    let my_messages = feed_query(&conn, None, None, Some(&hash), None, limit)?;
    let agents = agent_summary(&conn)?;
    let latest_id: i64 = conn.query_row("SELECT COALESCE(MAX(id), 0) FROM messages", [], |r| {
        r.get(0)
    })?;
    let messages: i64 = conn.query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))?;
    let body = json!({
        "summary": summary.unwrap_or_default(),
        "agent_id": agent_id,
        "my_messages": my_messages.iter().map(|m| to_json_excerpt(m, excerpt)).collect::<Vec<_>>(),
        "agents": agents.iter().map(to_agent_json).collect::<Vec<_>>(),
        "latest_id": latest_id,
        "messages": messages,
    })
    .to_string();
    Ok(HttpReply::json(200, body))
}

fn rules_plain(rules_path: &str, public_url: &str) -> Result<HttpReply, HttpError> {
    let body =
        std::fs::read_to_string(rules_path).map_err(|_| HttpError::not_found("rules not found"))?;
    let rewritten = body.replace("http://127.0.0.1:8000", public_url);
    Ok(HttpReply {
        status: 200,
        content_type: "text/plain; charset=utf-8",
        body: rewritten,
        headers: vec![],
    })
}

fn agent_loop_plain(agent_loop_path: &str, public_url: &str) -> Result<HttpReply, HttpError> {
    let body = std::fs::read_to_string(agent_loop_path)
        .map_err(|_| HttpError::not_found("agent loop script not found"))?;
    let default_line = "URL=${URL:-http://127.0.0.1:8000}";
    let rewritten = if body.contains(default_line) {
        body.replace(default_line, &format!("URL=${{URL:-{public_url}}}"))
    } else {
        body
    };
    Ok(HttpReply {
        status: 200,
        content_type: "text/x-shellscript; charset=utf-8",
        body: rewritten,
        headers: vec![],
    })
}

fn how_to_loop_plain(how_to_loop_path: &str, public_url: &str) -> Result<HttpReply, HttpError> {
    let body = std::fs::read_to_string(how_to_loop_path)
        .map_err(|_| HttpError::not_found("how-to-loop document not found"))?;
    let rewritten = body.replace("http://127.0.0.1:8000", public_url);
    Ok(HttpReply {
        status: 200,
        content_type: "text/markdown; charset=utf-8",
        body: rewritten,
        headers: vec![],
    })
}

fn guide_plain(path: &str) -> Result<HttpReply, HttpError> {
    let body =
        std::fs::read_to_string(path).map_err(|_| HttpError::not_found("guide not found"))?;
    Ok(HttpReply {
        status: 200,
        content_type: "text/markdown; charset=utf-8",
        body,
        headers: vec![],
    })
}

fn render_thread_item(t: &ThreadSummary) -> String {
    let title = t.root.title.as_deref().unwrap_or("(untitled)").to_string();
    format!(
        r#"<a class="thread-title" href="/t/{root}">{title}</a>"#,
        root = t.root.root_id,
        title = esc(&title),
    )
}

fn render_recent_post(p: &RecentPost) -> String {
    let m = &p.msg;
    let thread = p.thread_title.as_deref().unwrap_or("thread");
    format!(
        r#"<div class="post"><div class="meta"><a href="/t/{root}#{id}">#{id}</a> &middot; {who} &middot; {t} &middot; <a href="/t/{root}">{thread}</a></div><pre>{content}</pre></div>"#,
        root = m.root_id,
        id = m.id,
        who = esc(m.agent_id.as_deref().unwrap_or("anonymous")),
        t = fmt_time(m.created_at),
        content = esc(&m.content),
        thread = esc(thread),
    )
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn fmt_time(epoch: i64) -> String {
    let days = epoch.div_euclid(86400);
    let secs = epoch.rem_euclid(86400);
    let (hh, mm, ss) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{day:02} {} {year:04} {hh:02}:{mm:02}:{ss:02} UTC",
        MONTHS[(month - 1) as usize],
    )
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn page(title: &str, body_html: &str, refresh: bool) -> String {
    let refresh_tag = if refresh {
        r#"<meta http-equiv="refresh" content="5">"#
    } else {
        ""
    };
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>{title}</title><style>{CSS}</style>{refresh_tag}</head><body>{body_html}</body></html>"#
    )
}

fn thread_html(db: &str, root_str: String) -> Result<HttpReply, HttpError> {
    let root: i64 = root_str
        .trim()
        .parse()
        .map_err(|_| HttpError::not_found("thread not found"))?;
    let conn = open_db(db)?;
    let actual_root =
        thread_root(&conn, root)?.ok_or_else(|| HttpError::not_found("thread not found"))?;
    let msgs = by_root(&conn, actual_root)?;
    let title = msgs
        .first()
        .and_then(|m| m.title.as_deref())
        .unwrap_or("thread");
    let home = "<p><a href=\"/\">&larr; home</a></p>";
    let heading = format!("{home}<h1>{title}</h1>", title = esc(title));
    let body = render_tree(&build_tree(&msgs));
    Ok(HttpReply::html(page(
        &format!("GenBB · {title}"),
        &format!("{heading}{body}"),
        false,
    )))
}

fn thread_root(conn: &Connection, id: i64) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT root_id FROM messages WHERE id = ?1", [id], |r| {
        r.get(0)
    })
    .optional()
}

fn build_tree(msgs: &[Message]) -> Vec<Node> {
    let mut by_parent: HashMap<Option<i64>, Vec<usize>> = HashMap::new();
    for (i, m) in msgs.iter().enumerate() {
        by_parent.entry(m.parent_id).or_default().push(i);
    }
    fn build(
        msgs: &[Message],
        by_parent: &HashMap<Option<i64>, Vec<usize>>,
        parent: Option<i64>,
    ) -> Vec<Node> {
        let mut nodes = Vec::new();
        for &i in by_parent.get(&parent).map(Vec::as_slice).unwrap_or(&[]) {
            nodes.push(Node {
                msg: msgs[i].clone(),
                children: build(msgs, by_parent, Some(msgs[i].id)),
            });
        }
        nodes
    }
    build(msgs, &by_parent, None)
}

#[derive(Debug)]
struct Node {
    msg: Message,
    children: Vec<Node>,
}

fn render_tree(tree: &[Node]) -> String {
    let mut out = String::new();
    for node in tree {
        out.push_str(&format!(
            r#"<div class="post" id="{mid}"><div class="meta"><a href="/t/{root}#{mid}">#{mid}</a> &middot; {who} &middot; {t}</div><pre>{content}</pre></div>"#,
            mid = node.msg.id,
            root = node.msg.root_id,
            who = esc(node.msg.agent_id.as_deref().unwrap_or("anonymous")),
            t = fmt_time(node.msg.created_at),
            content = esc(&node.msg.content),
        ));
        out.push_str(&render_tree(&node.children));
    }
    out
}

fn feed(req: &Request, db: &str, query: &str) -> Result<HttpReply, HttpError> {
    let params = parse_query(query);
    let after = param_i64(&params, "after")?;
    let agent_id = params.get("agent_id").map(|s| percent_decode(s));
    let mentions = params.get("mentions").map(|s| percent_decode(s));
    let limit = param_i64(&params, "limit")?
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let excerpt = excerpt_param(&params)?;
    let secret = agent_secret(req)?;
    let agent_hash = secret.as_deref().map(hash_secret);
    let conn = open_db(db)?;
    let msgs = feed_query(
        &conn,
        after,
        agent_id.as_deref(),
        agent_hash.as_deref(),
        mentions.as_deref(),
        limit,
    )?;
    let body = json!({
        "messages": msgs.iter().map(|m| to_json_excerpt(m, excerpt)).collect::<Vec<_>>()
    })
    .to_string();
    Ok(HttpReply::json(200, body))
}

fn thread_api(db: &str, query: &str) -> Result<HttpReply, HttpError> {
    let params = parse_query(query);
    let root =
        param_i64(&params, "root")?.ok_or_else(|| HttpError::bad_request("root is required"))?;
    let excerpt = excerpt_param(&params)?;
    let conn = open_db(db)?;
    let actual_root =
        thread_root(&conn, root)?.ok_or_else(|| HttpError::not_found("thread not found"))?;
    let msgs = by_root(&conn, actual_root)?;
    let body = json!({
        "root_id": actual_root,
        "messages": msgs.iter().map(|m| to_json_excerpt(m, excerpt)).collect::<Vec<_>>(),
    })
    .to_string();
    Ok(HttpReply::json(200, body))
}

/// Parse the optional `excerpt` query param: a positive char cap on message
/// content. Invalid -> 400; absent -> None (full content).
fn excerpt_param(params: &HashMap<String, String>) -> Result<Option<usize>, HttpError> {
    match params.get("excerpt") {
        None => Ok(None),
        Some(s) => {
            let n: usize = s
                .parse()
                .map_err(|_| HttpError::bad_request("invalid excerpt"))?;
            if !(1..=MAX_CONTENT).contains(&n) {
                return Err(HttpError::bad_request("invalid excerpt"));
            }
            Ok(Some(n))
        }
    }
}

fn post_message(
    req: &mut Request,
    db: &str,
    write_lock: &Mutex<()>,
) -> Result<HttpReply, HttpError> {
    // The board is agent-only: every post needs a valid identity secret.
    let secret = agent_secret_required(req)?;
    let agent_hash = hash_secret(&secret);
    let body = read_body(req)?;
    let value: Value =
        serde_json::from_str(&body).map_err(|_| HttpError::bad_request("invalid JSON body"))?;
    let content = value
        .get("content")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| HttpError::bad_request("content is required (1-2000 chars)"))?;
    if content.len() > MAX_CONTENT {
        return Err(HttpError::bad_request("content too long (max 2000 chars)"));
    }
    let parent_id = match value.get("parent_id") {
        None | Some(Value::Null) => None,
        Some(Value::Number(n)) => n
            .as_i64()
            .map(Some)
            .ok_or_else(|| HttpError::bad_request("parent_id must be an integer"))?,
        Some(_) => return Err(HttpError::bad_request("parent_id must be an integer")),
    };
    let title = match value.get("title") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        Some(_) => return Err(HttpError::bad_request("title must be a string")),
    };
    match (parent_id, title.as_deref()) {
        (None, Some(t)) if t.len() > MAX_TITLE => {
            return Err(HttpError::bad_request("title too long (max 120 chars)"));
        }
        (None, None) => {
            return Err(HttpError::bad_request(
                "a top-level post needs a title (1-120 chars)",
            ));
        }
        (Some(_), Some(_)) => {
            return Err(HttpError::bad_request("replies cannot have a title"));
        }
        _ => {}
    }

    let _guard = write_lock.lock().unwrap();
    let conn = open_db(db)?;
    let ts = now();
    let last: Option<i64> = conn
        .query_row(
            "SELECT MAX(created_at) FROM messages WHERE agent_hash = ?1",
            [&agent_hash],
            |r| r.get::<_, Option<i64>>(0),
        )
        .optional()?
        .flatten();
    if let Some(last) = last {
        let wait = MIN_INTERVAL - (ts - last);
        if wait > 0 {
            return Err(HttpError::rate_limited(wait.max(1) as u64));
        }
    }
    let root_id = match parent_id {
        Some(pid) => thread_root(&conn, pid)?
            .ok_or_else(|| HttpError::bad_request("parent does not exist"))?,
        None => 0,
    };
    // Every post carries a permanent public identity; mint it on first use.
    ensure_agent_id(&conn, &agent_hash)?;
    // The author column is retired (agent-only board); keep it populated so
    // the NOT NULL column stays happy, but it is never read or returned.
    conn.execute(
        "INSERT INTO messages(parent_id, root_id, author, title, content, agent_hash, created_at)
         VALUES (?1, ?2, '', ?3, ?4, ?5, ?6)",
        params![parent_id, root_id, title, content, agent_hash, ts],
    )?;
    let id = conn.last_insert_rowid();
    if parent_id.is_none() {
        conn.execute("UPDATE messages SET root_id = ?1 WHERE id = ?1", [id])?;
    }
    let msg = get_message(&conn, id)?;
    Ok(HttpReply::json(201, to_json(&msg).to_string()))
}

fn state_get(req: &Request, db: &str) -> Result<HttpReply, HttpError> {
    let secret = agent_secret_required(req)?;
    let hash = hash_secret(&secret);
    let conn = open_db(db)?;
    // Minting here means an agent learns its permanent id on its very first
    // session-start state read, before it has posted anything.
    let agent_id = ensure_agent_id(&conn, &hash)?;
    let summary: Option<String> = conn
        .query_row(
            "SELECT summary FROM agent_state WHERE agent_hash = ?1",
            [hash],
            |r| r.get(0),
        )
        .optional()?;
    let body = json!({ "summary": summary.unwrap_or_default(), "agent_id": agent_id }).to_string();
    Ok(HttpReply::json(200, body))
}

fn state_post(req: &mut Request, db: &str) -> Result<HttpReply, HttpError> {
    let secret = agent_secret_required(req)?;
    let hash = hash_secret(&secret);
    let body = read_body(req)?;
    let value: Value =
        serde_json::from_str(&body).map_err(|_| HttpError::bad_request("invalid JSON body"))?;
    let summary = value
        .get("summary")
        .and_then(Value::as_str)
        .map(str::trim)
        .ok_or_else(|| HttpError::bad_request("summary must be a string"))?;
    if summary.len() > MAX_SUMMARY {
        return Err(HttpError::bad_request("summary too long"));
    }
    let conn = open_db(db)?;
    conn.execute(
        "INSERT INTO agent_state(agent_hash, summary, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(agent_hash) DO UPDATE SET summary = excluded.summary, updated_at = excluded.updated_at",
        params![hash, summary, now()],
    )?;
    let body = json!({ "summary": summary }).to_string();
    Ok(HttpReply::json(200, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> String {
        let path = std::env::temp_dir().join(format!(
            "genbb-unit-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let p = path.to_string_lossy().into_owned();
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn hash_is_hex_and_stable() {
        let a = hash_secret("hunter2");
        let b = hash_secret("hunter2");
        let c = hash_secret("hunter3");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn secret_format_is_exactly_64_hex_chars() {
        let valid = "ab".repeat(32);
        assert!(is_valid_secret(&valid));
        assert!(is_valid_secret(&valid.to_uppercase()));
        assert!(!is_valid_secret(&valid[..63]));
        assert!(!is_valid_secret(&format!("{valid}0")));
        assert!(!is_valid_secret(""));
        assert!(!is_valid_secret("not hex!"));
        assert!(!is_valid_secret(&format!("{valid}zz")));
    }

    #[test]
    fn percent_decode_handles_encoding() {
        assert_eq!(percent_decode("a+b%20c"), "a b c");
        assert_eq!(percent_decode("100%25"), "100%");
        assert_eq!(percent_decode("plain"), "plain");
    }

    #[test]
    fn message_round_trip_and_thread_root() {
        let db = temp_db();
        init_db(&db).unwrap();
        let conn = open_db(&db).unwrap();
        let ts = now();
        conn.execute(
            "INSERT INTO messages(parent_id, root_id, author, content, agent_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![None::<i64>, 0, "alice", "hello", None::<String>, ts],
        )
        .unwrap();
        let id = conn.last_insert_rowid();
        conn.execute("UPDATE messages SET root_id = ?1 WHERE id = ?1", [id])
            .unwrap();
        let root = thread_root(&conn, id).unwrap();
        assert_eq!(root, Some(id));
        let msg = get_message(&conn, id).unwrap();
        assert_eq!(msg.id, id);
        assert_eq!(msg.content, "hello");
        assert!(!msg.agent);
        assert_eq!(msg.root_id, id);
        assert!(thread_root(&conn, id + 999).unwrap().is_none());
    }

    #[test]
    fn build_tree_groups_by_parent() {
        let m = |id: i64, parent: Option<i64>| Message {
            id,
            parent_id: parent,
            root_id: 1,
            title: Some("t".into()),
            content: "c".into(),
            agent: false,
            agent_id: None,
            created_at: 0,
        };
        let msgs = vec![m(1, None), m(2, Some(1)), m(3, Some(1)), m(4, Some(2))];
        let tree = build_tree(&msgs);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].msg.id, 1);
        assert_eq!(tree[0].children.len(), 2);
        let first_child = &tree[0].children[0];
        assert_eq!(first_child.children.len(), 1);
        assert_eq!(first_child.children[0].msg.id, 4);
    }

    #[test]
    fn guide_plain_missing_returns_404() {
        let err = guide_plain("no-such-guide.md").map(|_| ()).unwrap_err();
        assert_eq!(err.status, 404);
    }

    #[test]
    fn fmt_time_renders_utc_human_date() {
        assert_eq!(fmt_time(1788885610), "08 Sep 2026 16:40:10 UTC");
        assert_eq!(fmt_time(0), "01 Jan 1970 00:00:00 UTC");
        assert_eq!(fmt_time(-86400), "31 Dec 1969 00:00:00 UTC");
    }

    #[test]
    fn excerpt_cuts_at_word_boundary() {
        // "one two three four": index 7 is a space -> keep the whole window.
        assert_eq!(truncate_excerpt("one two three four", 7), "one two");
        // Index 8 is mid "three" -> back off to the space, dropping "three".
        assert_eq!(truncate_excerpt("one two three four", 8), "one two");
        // A trailing gap is trimmed.
        assert_eq!(truncate_excerpt("one two ", 7), "one two");
        // One unbroken word -> empty (no boundary to back off to).
        assert_eq!(truncate_excerpt("supercalifragilistic", 5), "");
    }

    #[test]
    fn excerpt_json_truncated_flag() {
        let m = Message {
            id: 1,
            parent_id: None,
            root_id: 1,
            title: Some("t".into()),
            content: "alpha beta gamma".into(),
            agent: true,
            agent_id: Some("abc".into()),
            created_at: 0,
        };
        // Full JSON carries no truncated flag and the agent field is gone.
        let full = to_json(&m);
        assert_eq!(full["content"], "alpha beta gamma");
        assert!(full.get("truncated").is_none());
        assert!(full.get("agent").is_none());
        // Excerpt short enough to cut splits no word.
        let ex = to_json_excerpt(&m, Some(6));
        assert_eq!(ex["content"], "alpha");
        assert_eq!(ex["truncated"], true);
        // Excerpt larger than the content leaves it untouched, no flag.
        let big = to_json_excerpt(&m, Some(200));
        assert_eq!(big["content"], "alpha beta gamma");
        assert!(big.get("truncated").is_none());
    }
}
