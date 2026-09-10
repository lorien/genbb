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
pub const DEFAULT_DB: &str = "board.db";
pub const DEFAULT_WORKERS: usize = 4;
pub const DEFAULT_RULES: &str = "rules.md";
pub const DEFAULT_AGENT_LOOP: &str = "scripts/agent-loop.sh";
pub const DEFAULT_HOW_TO_LOOP: &str = "docs/how-to-loop.md";
const DOC_RUN_GITHUB_ACTION_AGENT: &str = "docs/run-github-action-agent.md";
pub const MAX_CONTENT: usize = 2000;
pub const MAX_SUMMARY: usize = 10000;
pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;
pub const HOME_LIMIT: i64 = 10;
pub const MIN_INTERVAL: i64 = 5;
pub const MAX_TITLE: usize = 120;
pub const AGENT_HEADER: &str = "X-Agent-ID";
/// Public id of the root user (all zeros). Reserved: never minted for an agent.
pub const ROOT_ID: &str = "000000000000";
/// Sentinel identity key of the root user in the `agents` registry. It is not
/// a possible sha256 hex output, so no agent secret can ever hash to it.
pub const ROOT_HASH: &str = "root";
/// Bootstrap password file, relative to the daemon working directory.
pub const ROOT_PWD: &str = "var/root.pwd";
const SESSION_COOKIE: &str = "genbb_session";
const SESSION_TTL_SECS: i64 = 7 * 24 * 3600;
/// Random salt length for a root password, in bytes (32 hex chars).
const PWD_SALT_BYTES: usize = 16;
const MAX_BODY: usize = 65536;
const CSS: &str = "body{background:#111;color:#ddd;font-family:sans-serif;margin:2rem auto;max-width:640px}.site-nav{margin-bottom:1.5rem;overflow:hidden}.site-nav .brand{font-weight:bold}.nav-right{float:right}.post{border-left:2px solid #333;padding:.5rem 1rem;margin:.5rem 0}.meta{color:#888;font-size:.85rem}a{color:#6af}pre{white-space:pre-wrap;word-break:break-word}.thread-title{font-size:1.05rem}form.inline{display:inline}button.as-link{background:none;border:none;color:#6af;cursor:pointer;padding:0;font:inherit;text-decoration:underline}code{color:#9df;word-break:break-all}.err{color:#f88}";

#[derive(Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub root_id: i64,
    pub title: Option<String>,
    pub content: String,
    /// Kind of author: `"agent"` or `"root"`.
    pub author_kind: &'static str,
    /// Public, permanent identity of the poster (12 hex chars; the root user
    /// carries the reserved all-zeros id).
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
        "author_kind": msg.author_kind,
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
    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: 403,
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
        eprintln!("[error] internal: {e}");
        Self::internal("internal error")
    }
}

impl From<std::io::Error> for HttpError {
    fn from(e: std::io::Error) -> Self {
        eprintln!("[error] internal: {e}");
        Self::internal("internal error")
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
        root_pwd: &str,
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
        let sessions = Arc::new(Mutex::new(HashMap::new()));
        warn_stale_root_pwd(db_path, root_pwd);
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
                root_pwd: root_pwd.to_string(),
                sessions: Arc::clone(&sessions),
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
         CREATE TABLE IF NOT EXISTS users (
             username TEXT PRIMARY KEY,
             salt TEXT NOT NULL,
             hash TEXT NOT NULL,
             created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS allowed_agents (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             agent_hash TEXT NOT NULL UNIQUE,
             secret TEXT NOT NULL,
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

/// Warn at daemon startup when a root password file lingers next to an
/// already-initialized root record: the record wins and the file is ignored.
fn warn_stale_root_pwd(db: &str, root_pwd: &str) {
    if !std::path::Path::new(root_pwd).exists() {
        return;
    }
    let conn = match open_db(db) {
        Ok(c) => c,
        Err(_) => return,
    };
    let has_root = conn
        .query_row("SELECT 1 FROM users WHERE username = 'root'", [], |r| {
            r.get::<_, i64>(0)
        })
        .optional()
        .map(|o| o.is_some())
        .unwrap_or(false);
    if has_root {
        eprintln!(
            "[warn] {root_pwd} exists but root is already initialized; ignoring the file (delete it)"
        );
    }
}

/// `bytes` random bytes from the OS entropy source, as lowercase hex.
fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let mut f = std::fs::File::open("/dev/urandom").expect("open /dev/urandom");
    f.read_exact(&mut buf).expect("read /dev/urandom");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// 12-hex public agent id: 6 random bytes from the OS entropy source.
fn random_agent_id() -> String {
    random_hex(AGENT_ID_BYTES)
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
        // The all-zeros id is reserved for the root user; never mint it.
        if id == ROOT_ID {
            continue;
        }
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
/// are accepted case-insensitively; case carries no identity (ADR-0014).
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

/// Identity hash for an agent secret: `hash_secret` of the lowercased secret.
/// Hex case is insignificant, so one secret in any case maps to one identity
/// (ADR-0014). Not for passwords, which stay case-sensitive.
pub fn hash_agent_secret(secret: &str) -> String {
    hash_secret(&secret.to_ascii_lowercase())
}

/// Password hash: sha256 of `salt:password`, hex. The stored credential is
/// `{salt}:{hash}`, where the salt is 16 random bytes as 32 hex chars.
fn pwd_hash(salt: &str, password: &str) -> String {
    hash_secret(&format!("{salt}:{password}"))
}

/// A parsed `{salt}:{hash}` credential record.
struct PwdRecord {
    salt: String,
    hash: String,
}

/// Parse and shape-validate a `{salt}:{hash}` credential line: salt is 32 hex
/// chars, hash 64 hex chars.
fn parse_pwd_file(content: &str) -> Result<PwdRecord, HttpError> {
    let (salt, hash) = content
        .trim()
        .split_once(':')
        .ok_or_else(|| HttpError::internal("malformed credential file: expected salt:hash"))?;
    let valid_hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_hexdigit());
    if !valid_hex(salt, 2 * PWD_SALT_BYTES) {
        return Err(HttpError::internal("malformed credential file: bad salt"));
    }
    if !valid_hex(hash, 64) {
        return Err(HttpError::internal("malformed credential file: bad hash"));
    }
    Ok(PwdRecord {
        salt: salt.to_string(),
        hash: hash.to_string(),
    })
}

fn row_to_message(r: &Row<'_>) -> rusqlite::Result<Message> {
    let agent_hash: Option<String> = r.get("agent_hash")?;
    Ok(Message {
        id: r.get("id")?,
        parent_id: r.get("parent_id")?,
        root_id: r.get("root_id")?,
        title: r.get("title")?,
        content: r.get("content")?,
        author_kind: if agent_hash.as_deref() == Some(ROOT_HASH) {
            "root"
        } else {
            "agent"
        },
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
    root_pwd: String,
    /// In-memory session store: token -> expiry epoch. Lost on restart.
    sessions: Arc<Mutex<HashMap<String, i64>>>,
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
        (Method::Get, "/") => index_html(&cfg.db, session_logged_in(&cfg.sessions, req)),
        (Method::Get, "/rules") => rules_plain(&cfg.rules, &cfg.public_url),
        (Method::Get, "/agent-loop.sh") => agent_loop_plain(&cfg.agent_loop, &cfg.public_url),
        (Method::Get, "/how-to-loop") => how_to_loop_plain(&cfg.how_to_loop, &cfg.public_url),
        (Method::Get, "/run-github-action-agent") => guide_plain(DOC_RUN_GITHUB_ACTION_AGENT),
        (Method::Get, p) if p.starts_with("/t/") => thread_html(
            &cfg.db,
            percent_decode(&p[3..]),
            session_logged_in(&cfg.sessions, req),
        ),
        (Method::Get, "/user/login") => user_login_form(cfg, req),
        (Method::Post, "/user/login") => user_login_post(req, cfg),
        (Method::Post, "/user/logout") => user_logout(cfg, req),
        (Method::Get, "/user/post") => user_post_form(cfg, req, query),
        (Method::Post, "/user/post") => user_post(req, cfg, write_lock),
        (Method::Get, "/user/agents") => user_agents_form(cfg, req),
        (Method::Post, "/user/agents") => user_agents_add(req, cfg),
        (Method::Get, "/user/agents/delete") => user_agents_delete_form(cfg, req, query),
        (Method::Post, "/user/agents/delete") => user_agents_delete(req, cfg),
        (Method::Get, "/api/messages") => feed(req, &cfg.db, query),
        (Method::Get, "/api/head") => head_json(req, &cfg.db),
        (Method::Get, "/api/agents") => agents_json(req, &cfg.db),
        (Method::Get, "/api/thread") => thread_api(req, &cfg.db, query),
        (Method::Get, "/api/state") => state_get(req, &cfg.db),
        (Method::Get, "/api/session") => session_json(req, &cfg.db, query),
        (Method::Post, "/api/messages") => post_message(req, &cfg.db, write_lock),
        (Method::Post, "/api/state") => state_post(req, &cfg.db),
        _ => Err(HttpError::not_found("not found")),
    }
}

/// A redirect reply: status plus a `Location` header.
fn redirect(location: &str, status: u16) -> HttpReply {
    HttpReply {
        status,
        content_type: "text/html; charset=utf-8",
        body: String::new(),
        headers: vec![("Location".to_string(), location.to_string())],
    }
}

/// An HTML page with an explicit status (the user pages under `/user/*`).
fn user_page(title: &str, body_html: &str, status: u16, logged_in: bool) -> HttpReply {
    HttpReply {
        status,
        content_type: "text/html; charset=utf-8",
        body: page(title, body_html, logged_in),
        headers: vec![],
    }
}

/// The human-readable author label shown on HTML pages: the root user is
/// `root`; every agent renders as `agent:<id>`; a legacy row with no identity
/// falls back to `anonymous`.
fn author_label(m: &Message) -> String {
    if m.author_kind == "root" {
        "root".to_string()
    } else if let Some(id) = m.agent_id.as_deref() {
        format!("agent:{id}")
    } else {
        "anonymous".to_string()
    }
}

/// Extract the session cookie value, if present.
fn session_token(req: &Request) -> Option<String> {
    let cookie = header_value(req, "Cookie")?;
    cookie.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        (k.trim().eq_ignore_ascii_case(SESSION_COOKIE)).then(|| v.trim().to_string())
    })
}

/// Whether the request carries a live session. Expired tokens are dropped.
fn session_logged_in(sessions: &Mutex<HashMap<String, i64>>, req: &Request) -> bool {
    let Some(token) = session_token(req) else {
        return false;
    };
    let mut map = sessions.lock().unwrap();
    match map.get(&token) {
        Some(&exp) if exp > now() => true,
        _ => {
            map.remove(&token);
            false
        }
    }
}

/// Mint a new session token with a fixed 7-day lifetime. Expired tokens are
/// swept on the way in, so tokens that were never presented again after
/// expiring do not pile up forever in the map.
fn session_create(sessions: &Mutex<HashMap<String, i64>>) -> String {
    let mut map = sessions.lock().unwrap();
    map.retain(|_, exp| *exp > now());
    let token = random_hex(32);
    map.insert(token.clone(), now() + SESSION_TTL_SECS);
    token
}

fn session_set_cookie(token: &str, secure: bool) -> (String, String) {
    let secure = if secure { "; Secure" } else { "" };
    (
        "Set-Cookie".to_string(),
        format!(
            "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECS}{secure}"
        ),
    )
}

fn session_clear_cookie() -> (String, String) {
    (
        "Set-Cookie".to_string(),
        format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"),
    )
}

fn root_initialized(db: &str, root_pwd: &str) -> Result<bool, HttpError> {
    let conn = open_db(db)?;
    let has_record = conn
        .query_row("SELECT 1 FROM users WHERE username = 'root'", [], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
        .is_some();
    Ok(has_record || std::path::Path::new(root_pwd).exists())
}

fn not_initialized_html(root_pwd: &str) -> String {
    format!(
        "<p><a href=\"/\">&larr; home</a></p>\
         <h1>Root is not initialized</h1>\
         <p>The password file <code>{root_pwd}</code> is missing and no root user record is in \
         the database. Create the file (salt:hash) as described in the README and try again.</p>"
    )
}

fn login_form_html(error: Option<&str>, login: &str) -> String {
    let err = error
        .map(|e| format!("<p style=\"color:#f88\">{}</p>", esc(e)))
        .unwrap_or_default();
    format!(
        "<p><a href=\"/\">&larr; home</a></p>\
         <h1>User login</h1>\
         {err}\
         <form method=\"post\" action=\"/user/login\">\
         <p><label>Login <input type=\"text\" name=\"login\" value=\"{login}\"></label></p>\
         <p><label>Password <input type=\"password\" name=\"password\"></label></p>\
         <p><button type=\"submit\">Sign in</button></p>\
         </form>",
        login = esc(login),
    )
}

/// `GET /user/login` — the login form (redirects away when already signed in).
fn user_login_form(cfg: &BoardConfig, req: &Request) -> Result<HttpReply, HttpError> {
    if session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/", 302));
    }
    if !root_initialized(&cfg.db, &cfg.root_pwd)? {
        return Ok(user_page(
            "GenBB",
            &not_initialized_html(&cfg.root_pwd),
            200,
            false,
        ));
    }
    Ok(user_page(
        "GenBB · login",
        &login_form_html(None, ""),
        200,
        false,
    ))
}

/// `POST /user/login` — verify credentials, create a session, and on the very
/// first successful login migrate the bootstrap password file into the
/// database under a fresh random salt, then erase the file.
fn user_login_post(req: &mut Request, cfg: &BoardConfig) -> Result<HttpReply, HttpError> {
    if session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/", 302));
    }
    let body = read_body(req)?;
    let form = parse_query(&body);
    let login = form.get("login").cloned().unwrap_or_default();
    let password = form.get("password").cloned().unwrap_or_default();

    // A single, uniform failure for any wrong credential: never reveal whether
    // the login or the password was the bad part.
    let failed = |login: &str| {
        user_page(
            "GenBB · login",
            &login_form_html(Some("invalid login or password"), login),
            401,
            false,
        )
    };

    if login != "root" {
        return Ok(failed(&login));
    }

    if !root_initialized(&cfg.db, &cfg.root_pwd)? {
        return Ok(user_page(
            "GenBB",
            &not_initialized_html(&cfg.root_pwd),
            503,
            false,
        ));
    }

    let conn = open_db(&cfg.db)?;
    let record: Option<(String, String)> = conn
        .query_row(
            "SELECT salt, hash FROM users WHERE username = 'root'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;

    let (salt, hash, from_file) = match record {
        Some((s, h)) => (s, h, false),
        None => {
            let content = std::fs::read_to_string(&cfg.root_pwd).map_err(|_| {
                HttpError::internal("root is not initialized: password file missing")
            })?;
            let parsed = parse_pwd_file(&content)?;
            (parsed.salt, parsed.hash, true)
        }
    };

    if pwd_hash(&salt, &password) != hash {
        return Ok(failed(&login));
    }

    if from_file {
        // First successful login from the bootstrap file: store the credential
        // in the database under a new random salt and drop the plaintext-ish
        // file. INSERT OR IGNORE keeps a concurrent first login safe.
        let new_salt = random_hex(PWD_SALT_BYTES);
        let new_hash = pwd_hash(&new_salt, &password);
        conn.execute(
            "INSERT OR IGNORE INTO users(username, salt, hash, created_at)
             VALUES ('root', ?1, ?2, ?3)",
            params![new_salt, new_hash, now()],
        )?;
        if let Err(e) = std::fs::remove_file(&cfg.root_pwd) {
            eprintln!("[warn] could not remove {}: {e}", cfg.root_pwd);
        }
    }

    let token = session_create(&cfg.sessions);
    // The Rust server sits behind nginx, so it learns the real scheme from the
    // forwarded header. Behind TLS the cookie must be Secure; a plain local
    // dev run (no proxy, no header) keeps the flag off so http:// login works.
    let secure =
        header_value(req, "X-Forwarded-Proto").is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let mut reply = redirect("/", 303);
    reply.headers.push(session_set_cookie(&token, secure));
    Ok(reply)
}

/// `POST /user/logout` — drop the session and clear the cookie. POST-only:
/// logout is a state change, and a GET would let a cross-site top-level
/// navigation (which SameSite=Lax still attaches the cookie to) log root out.
fn user_logout(cfg: &BoardConfig, req: &Request) -> Result<HttpReply, HttpError> {
    if let Some(token) = session_token(req) {
        cfg.sessions.lock().unwrap().remove(&token);
    }
    let mut reply = redirect("/", 302);
    reply.headers.push(session_clear_cookie());
    Ok(reply)
}

/// HTML for the compose page: a reply form (when `parent` is set, with the
/// parent message shown above) or a new-thread form.
fn compose_page_html(
    db: &str,
    parent: Option<i64>,
    title: &str,
    content: &str,
    error: Option<&str>,
) -> Result<String, HttpError> {
    let err = error
        .map(|e| format!("<p style=\"color:#f88\">{}</p>", esc(e)))
        .unwrap_or_default();
    match parent {
        Some(pid) => {
            let conn = open_db(db)?;
            let root =
                thread_root(&conn, pid)?.ok_or_else(|| HttpError::not_found("thread not found"))?;
            let msgs = by_root(&conn, root)?;
            let pm = msgs
                .iter()
                .find(|m| m.id == pid)
                .cloned()
                .ok_or_else(|| HttpError::not_found("parent not found"))?;
            let thread_title = msgs
                .first()
                .and_then(|m| m.title.as_deref())
                .unwrap_or("thread")
                .to_string();
            Ok(format!(
                r#"<p><a href="/">home</a> &middot; <a href="/t/{root}">&larr; {thread_title}</a></p>
                   <h1>Reply to #{pid}</h1>
                   <div class="post"><div class="meta">{who} &middot; {time}</div><pre>{pcontent}</pre></div>
                   {err}
                   <form method="post" action="/user/post">
                   <input type="hidden" name="parent" value="{pid}">
                   <p><textarea name="content" rows="6" cols="60">{content}</textarea></p>
                   <p><button type="submit">Post reply</button></p>
                   </form>"#,
                root = root,
                thread_title = esc(&thread_title),
                pid = pid,
                who = esc(&author_label(&pm)),
                time = fmt_time(pm.created_at),
                pcontent = esc(&pm.content),
                content = esc(content),
            ))
        }
        None => Ok(format!(
            r#"<p><a href="/">home</a></p>
               <h1>Create a new thread</h1>
               {err}
               <form method="post" action="/user/post">
               <p><label>Title <input type="text" name="title" value="{title}" size="60" maxlength="{MAX_TITLE}"></label></p>
               <p><textarea name="content" rows="6" cols="60">{content}</textarea></p>
               <p><button type="submit">Post</button></p>
               </form>"#,
            title = esc(title),
            content = esc(content),
        )),
    }
}

/// `GET /user/post` — the compose page (login required).
fn user_post_form(cfg: &BoardConfig, req: &Request, query: &str) -> Result<HttpReply, HttpError> {
    if !session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/user/login", 302));
    }
    let params = parse_query(query);
    let parent = param_i64(&params, "parent")?;
    let body = compose_page_html(&cfg.db, parent, "", "", None)?;
    Ok(user_page("GenBB · post", &body, 200, true))
}

/// `POST /user/post` — create a message as the root user (login required).
/// Root has no per-identity rate limit. On success, redirect to the thread at
/// the new post so the author lands right on their answer.
fn user_post(
    req: &mut Request,
    cfg: &BoardConfig,
    write_lock: &Mutex<()>,
) -> Result<HttpReply, HttpError> {
    if !session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/user/login", 302));
    }
    let body = read_body(req)?;
    let form = parse_query(&body);
    let raw_content = form.get("content").cloned().unwrap_or_default();
    let raw_title = form.get("title").cloned().unwrap_or_default();
    let parent = match form.get("parent") {
        None => None,
        Some(s) if s.trim().is_empty() => None,
        Some(s) => Some(
            s.trim()
                .parse::<i64>()
                .map_err(|_| HttpError::bad_request("invalid parent"))?,
        ),
    };
    let content = raw_content.trim();
    let title = raw_title.trim();

    // A reply target that does not exist is a hard form error.
    if let Some(pid) = parent {
        let conn = open_db(&cfg.db)?;
        if thread_root(&conn, pid)?.is_none() {
            let page = compose_page_html(
                &cfg.db,
                None,
                &raw_title,
                &raw_content,
                Some("parent does not exist"),
            )?;
            return Ok(user_page("GenBB · post", &page, 400, true));
        }
    }

    // Validation mirrors POST /api/messages.
    let error: Option<&str> = if content.is_empty() || content.len() > MAX_CONTENT {
        Some("content is required (1-2000 chars)")
    } else if parent.is_none() && title.is_empty() {
        Some("a top-level post needs a title (1-120 chars)")
    } else if parent.is_none() && title.len() > MAX_TITLE {
        Some("title too long (max 120 chars)")
    } else if parent.is_some() && !title.is_empty() {
        Some("replies cannot have a title")
    } else {
        None
    };
    if let Some(e) = error {
        let page = compose_page_html(&cfg.db, parent, &raw_title, &raw_content, Some(e))?;
        return Ok(user_page("GenBB · post", &page, 400, true));
    }

    let _guard = write_lock.lock().unwrap();
    let conn = open_db(&cfg.db)?;
    let ts = now();
    // The root user's public identity: the sentinel registry row, minted on
    // first post (same lazy rule as agents).
    conn.execute(
        "INSERT OR IGNORE INTO agents(agent_hash, agent_id, created_at) VALUES (?1, ?2, ?3)",
        params![ROOT_HASH, ROOT_ID, ts],
    )?;
    let root_id = match parent {
        Some(pid) => thread_root(&conn, pid)?
            .ok_or_else(|| HttpError::bad_request("parent does not exist"))?,
        None => 0,
    };
    conn.execute(
        "INSERT INTO messages(parent_id, root_id, author, title, content, agent_hash, created_at)
         VALUES (?1, ?2, '', ?3, ?4, ?5, ?6)",
        params![
            parent,
            root_id,
            (!title.is_empty()).then(|| title.to_string()),
            content,
            ROOT_HASH,
            ts
        ],
    )?;
    let id = conn.last_insert_rowid();
    let final_root = if parent.is_none() {
        conn.execute("UPDATE messages SET root_id = ?1 WHERE id = ?1", [id])?;
        id
    } else {
        root_id
    };
    Ok(redirect(&format!("/t/{final_root}#{id}"), 303))
}

/// One row of the agent allowlist, joined to the public id when the agent has
/// already been seen. `agent_id` is None until the agent first calls in.
struct AllowedAgent {
    id: i64,
    secret: String,
    created_at: i64,
    agent_id: Option<String>,
}

fn row_to_allowed(r: &rusqlite::Row) -> rusqlite::Result<AllowedAgent> {
    Ok(AllowedAgent {
        id: r.get("id")?,
        secret: r.get("secret")?,
        created_at: r.get("created_at")?,
        agent_id: r.get("agent_id")?,
    })
}

const ALLOWED_SELECT: &str = "SELECT al.id, al.secret, al.created_at, a.agent_id
     FROM allowed_agents al
     LEFT JOIN agents a ON a.agent_hash = al.agent_hash";

fn allowed_agents(conn: &Connection) -> rusqlite::Result<Vec<AllowedAgent>> {
    conn.prepare(&format!("{ALLOWED_SELECT} ORDER BY al.id"))?
        .query_map([], row_to_allowed)?
        .collect()
}

fn allowed_agent(conn: &Connection, id: i64) -> rusqlite::Result<Option<AllowedAgent>> {
    conn.query_row(
        &format!("{ALLOWED_SELECT} WHERE al.id = ?1"),
        [id],
        row_to_allowed,
    )
    .optional()
}

fn agents_page_html(rows: &[AllowedAgent], error: Option<&str>) -> String {
    let err = error
        .map(|e| format!("<p class=\"err\">{}</p>", esc(e)))
        .unwrap_or_default();
    let list = if rows.is_empty() {
        "<p>No agents are allowed yet; the API rejects every secret.</p>".to_string()
    } else {
        let mut items = String::from("<ul>");
        for r in rows {
            let agent = r.agent_id.as_deref().unwrap_or("not seen yet");
            items.push_str(&format!(
                "<li>#{id} &middot; {agent} &middot; <code>{secret}</code> \
                 &middot; added {time} &middot; \
                 <a href=\"/user/agents/delete?id={id}\">delete</a></li>",
                id = r.id,
                agent = esc(agent),
                secret = esc(&r.secret),
                time = fmt_time(r.created_at),
            ));
        }
        items.push_str("</ul>");
        items
    };
    format!(
        r#"<p><a href="/">home</a></p>
           <h1>Allowed agents</h1>
           <p>A secret on this list may use the JSON API. Deleting an entry
              revokes access but keeps the agent's posts and state.</p>
           {err}
           {list}
           <h2>Add an agent</h2>
           <form method="post" action="/user/agents">
           <p><label>Secret <input type="text" name="secret" size="70" maxlength="64"
              placeholder="paste a 64-hex secret, or generate one"></label></p>
           <p><label><input type="checkbox" name="generate" value="1">
              generate a new secret</label></p>
           <p><button type="submit">Add</button></p>
           </form>"#
    )
}

/// `GET /user/agents` — the allowlist page (login required).
fn user_agents_form(cfg: &BoardConfig, req: &Request) -> Result<HttpReply, HttpError> {
    if !session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/user/login", 302));
    }
    let conn = open_db(&cfg.db)?;
    let rows = allowed_agents(&conn)?;
    Ok(user_page(
        "GenBB · agents",
        &agents_page_html(&rows, None),
        200,
        true,
    ))
}

/// Re-render the allowlist page with an error (400).
fn agents_page_error(cfg: &BoardConfig, message: &str) -> Result<HttpReply, HttpError> {
    let conn = open_db(&cfg.db)?;
    let rows = allowed_agents(&conn)?;
    Ok(user_page(
        "GenBB · agents",
        &agents_page_html(&rows, Some(message)),
        400,
        true,
    ))
}

/// `POST /user/agents` — add or re-add an allowed agent (login required). A
/// pasted secret is validated and canonicalized to lowercase; otherwise a
/// fresh secret is generated. Idempotent: re-adding refreshes the stored form.
fn user_agents_add(req: &mut Request, cfg: &BoardConfig) -> Result<HttpReply, HttpError> {
    if !session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/user/login", 302));
    }
    let body = read_body(req)?;
    let form = parse_query(&body);
    let pasted = form.get("secret").map(|s| s.trim()).unwrap_or_default();
    let secret = if !pasted.is_empty() {
        if !is_valid_secret(pasted) {
            return agents_page_error(cfg, "a secret must be 64 hex chars (openssl rand -hex 32)");
        }
        pasted.to_ascii_lowercase()
    } else if form.contains_key("generate") {
        random_hex(32)
    } else {
        return agents_page_error(cfg, "paste a 64-hex secret, or tick generate");
    };
    let hash = hash_agent_secret(&secret);
    let conn = open_db(&cfg.db)?;
    conn.execute(
        "INSERT INTO allowed_agents(agent_hash, secret, created_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(agent_hash) DO UPDATE SET secret = excluded.secret",
        params![hash, secret, now()],
    )?;
    Ok(redirect("/user/agents", 303))
}

/// `GET /user/agents/delete` — confirmation page for revoking one entry.
fn user_agents_delete_form(
    cfg: &BoardConfig,
    req: &Request,
    query: &str,
) -> Result<HttpReply, HttpError> {
    if !session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/user/login", 302));
    }
    let params = parse_query(query);
    let id = param_i64(&params, "id")?.ok_or_else(|| HttpError::bad_request("id is required"))?;
    let conn = open_db(&cfg.db)?;
    let Some(row) = allowed_agent(&conn, id)? else {
        return Ok(user_page(
            "GenBB · agents",
            "<p>No such allowlist entry.</p>",
            404,
            true,
        ));
    };
    let agent = row.agent_id.as_deref().unwrap_or("not seen yet");
    let body = format!(
        r#"<p><a href="/user/agents">&larr; allowed agents</a></p>
           <h1>Delete allowlist entry #{id}</h1>
           <p>{agent} &middot; <code>{secret}</code></p>
           <p>Its posts and private state are kept, but the secret stops
              working on the API.</p>
           <form method="post" action="/user/agents/delete">
           <input type="hidden" name="id" value="{id}">
           <p><button type="submit">Delete</button></p>
           </form>"#,
        id = row.id,
        agent = esc(agent),
        secret = esc(&row.secret),
    );
    Ok(user_page("GenBB · delete agent", &body, 200, true))
}

/// `POST /user/agents/delete` — revoke one allowlist entry (login required).
/// Only the allowlist row goes; posts and private state are untouched.
fn user_agents_delete(req: &mut Request, cfg: &BoardConfig) -> Result<HttpReply, HttpError> {
    if !session_logged_in(&cfg.sessions, req) {
        return Ok(redirect("/user/login", 302));
    }
    let body = read_body(req)?;
    let form = parse_query(&body);
    let id = form
        .get("id")
        .and_then(|s| s.trim().parse::<i64>().ok())
        .ok_or_else(|| HttpError::bad_request("id is required"))?;
    let conn = open_db(&cfg.db)?;
    conn.execute("DELETE FROM allowed_agents WHERE id = ?1", [id])?;
    Ok(redirect("/user/agents", 303))
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

/// The board is invite-only: a well-formed secret is necessary but not
/// sufficient. 403 when `hash` is not on the allowlist.
fn ensure_allowed(conn: &Connection, hash: &str) -> Result<(), HttpError> {
    let allowed: bool = conn
        .query_row(
            "SELECT 1 FROM allowed_agents WHERE agent_hash = ?1",
            [hash],
            |_| Ok(true),
        )
        .optional()?
        .unwrap_or(false);
    if allowed {
        Ok(())
    } else {
        Err(HttpError::forbidden("agent not allowed"))
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

/// An HTML page that must never be cached: its markup depends on the viewer's
/// session state (reply/logout links), so a shared cache could hand one
/// visitor's logged-in view to another.
fn uncached_html(body: String) -> HttpReply {
    let mut reply = HttpReply::html(body);
    reply
        .headers
        .push(("Cache-Control".to_string(), "no-store".to_string()));
    reply
}

fn index_html(db: &str, logged_in: bool) -> Result<HttpReply, HttpError> {
    let conn = open_db(db)?;
    let threads = recent_threads(&conn, HOME_LIMIT)?;
    let thread_items = threads
        .iter()
        .map(render_thread_item)
        .collect::<Vec<_>>()
        .join(" \u{2022} ");
    let posts = recent_posts(&conn, HOME_LIMIT)?;
    let post_items = posts
        .iter()
        .map(|p| render_recent_post(p, logged_in))
        .collect::<String>();
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
    Ok(uncached_html(page(
        "GenBB",
        &format!("{top}{empty}{threads_html}{posts_html}"),
        logged_in,
    )))
}

fn agents_json(req: &Request, db: &str) -> Result<HttpReply, HttpError> {
    let hash = hash_agent_secret(&agent_secret_required(req)?);
    let conn = open_db(db)?;
    ensure_allowed(&conn, &hash)?;
    let agents = agent_summary(&conn)?;
    let body =
        json!({ "agents": agents.iter().map(to_agent_json).collect::<Vec<_>>() }).to_string();
    Ok(HttpReply::json(200, body))
}

/// Cheap board-head: latest message id plus board counts. A one-line poll an
/// agent can issue every cycle to learn whether anything is new before paying
/// for a full feed read.
fn head_json(req: &Request, db: &str) -> Result<HttpReply, HttpError> {
    let hash = hash_agent_secret(&agent_secret_required(req)?);
    let conn = open_db(db)?;
    ensure_allowed(&conn, &hash)?;
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
        "kind": if a.agent_id == ROOT_ID { "root" } else { "agent" },
        "posts": a.posts,
        "last_seen": a.last_seen,
    })
}

/// One-round-trip session start: an agent's own state, own posts, who is
/// around, and the board head. Replaces the overlapping feed/state/agents
/// fetches agents were doing each cycle.
fn session_json(req: &Request, db: &str, query: &str) -> Result<HttpReply, HttpError> {
    let secret = agent_secret_required(req)?;
    let hash = hash_agent_secret(&secret);
    let conn = open_db(db)?;
    ensure_allowed(&conn, &hash)?;
    let params = parse_query(query);
    let limit = param_i64(&params, "limit")?
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let excerpt = excerpt_param(&params)?;
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
    let rewritten = body.replace("http://127.0.0.1:8065", public_url);
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
    let default_line = "URL=${URL:-http://127.0.0.1:8065}";
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
    let rewritten = body.replace("http://127.0.0.1:8065", public_url);
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

fn render_recent_post(p: &RecentPost, logged_in: bool) -> String {
    let m = &p.msg;
    let thread = p.thread_title.as_deref().unwrap_or("thread");
    format!(
        r#"<div class="post"><div class="meta"><a href="/t/{root}#{id}">#{id}</a> &middot; {who} &middot; {t}{reply} &middot; <a href="/t/{root}">{thread}</a></div><pre>{content}</pre></div>"#,
        root = m.root_id,
        id = m.id,
        who = esc(&author_label(m)),
        t = fmt_time(m.created_at),
        reply = reply_link(m.id, logged_in),
        content = esc(&m.content),
        thread = esc(thread),
    )
}

fn reply_link(id: i64, logged_in: bool) -> String {
    if logged_in {
        format!(r#" &middot; <a href="/user/post?parent={id}">reply</a>"#)
    } else {
        String::new()
    }
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

fn page(title: &str, body_html: &str, logged_in: bool) -> String {
    // The title lands in an HTML text context (<title>), so escape it. Titles
    // are agent-controlled (thread titles come straight from the DB here); a
    // raw title could close the tag and inject script into the page.
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><title>{title}</title><style>{CSS}</style></head><body>{nav}{body_html}</body></html>"#,
        title = esc(title),
        nav = nav_html(logged_in),
    )
}

fn nav_html(logged_in: bool) -> String {
    let right = if logged_in {
        r#"<a href="/user/agents">agents</a> \
           <a href="/user/post">create new thread</a> \
           <form class="inline" method="post" action="/user/logout"><button type="submit" class="as-link">logout</button></form>"#
    } else {
        r#"<a href="/user/login">login</a>"#
    };
    format!(
        r#"<nav class="site-nav"><a class="brand" href="/">GenBB</a><span class="nav-right">{right}</span></nav>"#
    )
}

fn thread_html(db: &str, root_str: String, logged_in: bool) -> Result<HttpReply, HttpError> {
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
    let home = "<div><a href=\"/\">&larr; home</a></div>";
    let heading = format!("{home}<h1>{title}</h1>", title = esc(title));
    let body = render_tree(&build_tree(&msgs), logged_in);
    Ok(uncached_html(page(
        &format!("GenBB · {title}"),
        &format!("{heading}{body}"),
        logged_in,
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

fn render_tree(tree: &[Node], logged_in: bool) -> String {
    let mut out = String::new();
    for node in tree {
        out.push_str(&format!(
            r#"<div class="post" id="{mid}"><div class="meta"><a href="/t/{root}#{mid}">#{mid}</a> &middot; {who} &middot; {t}{reply}</div><pre>{content}</pre></div>"#,
            mid = node.msg.id,
            root = node.msg.root_id,
            who = esc(&author_label(&node.msg)),
            t = fmt_time(node.msg.created_at),
            reply = reply_link(node.msg.id, logged_in),
            content = esc(&node.msg.content),
        ));
        out.push_str(&render_tree(&node.children, logged_in));
    }
    out
}

fn feed(req: &Request, db: &str, query: &str) -> Result<HttpReply, HttpError> {
    let agent_hash = hash_agent_secret(&agent_secret_required(req)?);
    let conn = open_db(db)?;
    ensure_allowed(&conn, &agent_hash)?;
    let params = parse_query(query);
    let after = param_i64(&params, "after")?;
    let agent_id = params.get("agent_id").map(|s| percent_decode(s));
    let mentions = params.get("mentions").map(|s| percent_decode(s));
    let limit = param_i64(&params, "limit")?
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let excerpt = excerpt_param(&params)?;
    // The identity header is now mandatory, so it can no longer also mean
    // "my posts only" (that would leave agents unable to read the board).
    // Own posts come from /api/session or ?agent_id=<own id>.
    let msgs = feed_query(
        &conn,
        after,
        agent_id.as_deref(),
        None,
        mentions.as_deref(),
        limit,
    )?;
    let body = json!({
        "messages": msgs.iter().map(|m| to_json_excerpt(m, excerpt)).collect::<Vec<_>>()
    })
    .to_string();
    Ok(HttpReply::json(200, body))
}

fn thread_api(req: &Request, db: &str, query: &str) -> Result<HttpReply, HttpError> {
    let hash = hash_agent_secret(&agent_secret_required(req)?);
    let conn = open_db(db)?;
    ensure_allowed(&conn, &hash)?;
    let params = parse_query(query);
    let root =
        param_i64(&params, "root")?.ok_or_else(|| HttpError::bad_request("root is required"))?;
    let excerpt = excerpt_param(&params)?;
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
    let agent_hash = hash_agent_secret(&secret);
    // Invite-only: reject an unlisted secret before reading the body, so the
    // ordering is 400/401/403 before any content validation errors.
    {
        let conn = open_db(db)?;
        ensure_allowed(&conn, &agent_hash)?;
    }
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
    let hash = hash_agent_secret(&secret);
    let conn = open_db(db)?;
    ensure_allowed(&conn, &hash)?;
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
    let hash = hash_agent_secret(&secret);
    let conn = open_db(db)?;
    ensure_allowed(&conn, &hash)?;
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

    #[test]
    fn internal_errors_are_generic() {
        let db = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("secret db detail".to_string()),
        );
        let he = HttpError::from(db);
        assert_eq!(he.status, 500);
        assert_eq!(he.message, "internal error");
        assert!(!he.message.contains("secret db detail"));

        let io = std::io::Error::other("secret io detail");
        let he = HttpError::from(io);
        assert_eq!(he.status, 500);
        assert_eq!(he.message, "internal error");
        assert!(!he.message.contains("secret io detail"));
    }

    #[test]
    fn session_create_sweeps_expired_tokens() {
        let sessions = Mutex::new(HashMap::new());
        {
            let mut map = sessions.lock().unwrap();
            map.insert("expired".to_string(), now() - 1);
            map.insert("live".to_string(), now() + SESSION_TTL_SECS);
        }
        session_create(&sessions);
        let map = sessions.lock().unwrap();
        assert!(!map.contains_key("expired"), "expired token was not swept");
        assert!(map.contains_key("live"), "live token was dropped");
        assert_eq!(map.len(), 2, "sweep must keep the live token + the new one");
    }

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
    fn agent_secret_hash_ignores_hex_case() {
        let lower = "ab".repeat(32);
        let upper = lower.to_uppercase();
        assert_eq!(hash_agent_secret(&lower), hash_agent_secret(&upper));
        assert_ne!(hash_agent_secret(&lower), hash_secret(&upper));
        // Canonical lowercase keeps the pre-ADR-0014 hash, so lowercase agents
        // retain their identity, state, and history.
        assert_eq!(hash_agent_secret(&lower), hash_secret(&lower));
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
        assert_eq!(msg.author_kind, "agent");
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
            author_kind: "agent",
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
            author_kind: "agent",
            agent_id: Some("abc".into()),
            created_at: 0,
        };
        // Full JSON carries no truncated flag, no retired agent field, and an
        // explicit author_kind.
        let full = to_json(&m);
        assert_eq!(full["content"], "alpha beta gamma");
        assert!(full.get("truncated").is_none());
        assert!(full.get("agent").is_none());
        assert_eq!(full["author_kind"], "agent");
        // Excerpt short enough to cut splits no word.
        let ex = to_json_excerpt(&m, Some(6));
        assert_eq!(ex["content"], "alpha");
        assert_eq!(ex["truncated"], true);
        // Excerpt larger than the content leaves it untouched, no flag.
        let big = to_json_excerpt(&m, Some(200));
        assert_eq!(big["content"], "alpha beta gamma");
        assert!(big.get("truncated").is_none());
    }

    #[test]
    fn pwd_hash_is_deterministic_and_salt_sensitive() {
        assert_eq!(pwd_hash("a1", "pw"), pwd_hash("a1", "pw"));
        assert_ne!(pwd_hash("a1", "pw"), pwd_hash("a2", "pw"));
        assert_ne!(pwd_hash("a1", "pw"), pwd_hash("a1", "px"));
        assert_eq!(pwd_hash("a1", "pw").len(), 64);
    }

    #[test]
    fn pwd_hash_stays_case_sensitive() {
        assert_ne!(pwd_hash("a1", "pw"), pwd_hash("a1", "PW"));
    }

    #[test]
    fn parse_pwd_file_validates_shape() {
        let salt = "ab".repeat(16);
        let hash = "cd".repeat(32);
        let good = format!("{salt}:{hash}\n");
        let rec = parse_pwd_file(&good).unwrap();
        assert_eq!(rec.salt, salt);
        assert_eq!(rec.hash, hash);
        assert!(parse_pwd_file("no-colon").is_err());
        assert!(parse_pwd_file(&format!("short:{hash}")).is_err());
        assert!(parse_pwd_file(&format!("{salt}:short")).is_err());
        assert!(parse_pwd_file(&format!("zz{salt}:{hash}")).is_err());
    }

    #[test]
    fn author_label_distinguishes_root_agents_and_anonymous() {
        let msg = |kind: &'static str, id: Option<String>| Message {
            id: 1,
            parent_id: None,
            root_id: 1,
            title: None,
            content: "c".into(),
            author_kind: kind,
            agent_id: id,
            created_at: 0,
        };
        assert_eq!(author_label(&msg("root", Some(ROOT_ID.into()))), "root");
        assert_eq!(
            author_label(&msg("agent", Some("abc123".into()))),
            "agent:abc123"
        );
        assert_eq!(author_label(&msg("agent", None)), "anonymous");
    }

    #[test]
    fn root_reserved_id_is_not_the_agent_format_width_mismatch() {
        // ROOT_HASH is not 64 hex chars, so it can never equal an agent's
        // sha256 hash; ROOT_ID is the reserved all-zeros 12-hex id.
        assert_eq!(ROOT_ID.len(), AGENT_ID_LEN);
        assert_eq!(ROOT_HASH.len(), 4);
        assert!(ROOT_HASH.bytes().any(|b| !b.is_ascii_hexdigit()));
    }
}
