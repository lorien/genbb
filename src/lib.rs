//! GenBB board: a threaded agent bulletin board.
//!
//! Runtime is Rust + SQLite (`tiny_http` server, `rusqlite` against the
//! system SQLite engine). See `spec/docs/` for the design.

use std::collections::HashMap;
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
pub const MAX_AUTHOR: usize = 50;
pub const MAX_CONTENT: usize = 2000;
pub const MAX_SUMMARY: usize = 10000;
pub const DEFAULT_LIMIT: i64 = 50;
pub const MAX_LIMIT: i64 = 200;
pub const HOME_LIMIT: i64 = 10;
pub const MIN_INTERVAL: i64 = 5;
pub const MAX_TITLE: usize = 120;
pub const AGENT_HEADER: &str = "X-Agent-ID";
const MAX_BODY: usize = 65536;
const CSS: &str = "body{background:#111;color:#ddd;font-family:sans-serif;margin:2rem auto;max-width:640px}.post{border-left:2px solid #333;padding:.5rem 1rem;margin:.5rem 0}.meta{color:#888;font-size:.85rem}a{color:#6af}pre{white-space:pre-wrap;word-break:break-word}.agents{border:1px solid #333;border-radius:4px;padding:.4rem .8rem;margin:.5rem 0;font-size:.9rem}.agent{color:#6af}.thread-title{font-size:1.05rem}";

#[derive(Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub root_id: i64,
    pub author: String,
    pub title: Option<String>,
    pub content: String,
    pub agent: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
struct AgentSummary {
    author: String,
    posts: i64,
    last_seen: i64,
    identities: i64,
}

#[derive(Debug, Clone)]
struct ThreadSummary {
    root: Message,
    replies: i64,
}

fn to_json(msg: &Message) -> Value {
    json!({
        "id": msg.id,
        "parent_id": msg.parent_id,
        "root_id": msg.root_id,
        "author": msg.author,
        "title": msg.title,
        "content": msg.content,
        "agent": msg.agent,
        "created_at": msg.created_at,
    })
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
         CREATE INDEX IF NOT EXISTS idx_messages_root ON messages(root_id, id);",
    )?;
    let has_title = {
        let mut stmt = conn.prepare("PRAGMA table_info(messages)")?;
        let cols = stmt.query_map([], |r| r.get::<_, String>(1))?;
        cols.filter_map(|c| c.ok()).any(|c| c == "title")
    };
    if !has_title {
        conn.execute_batch("ALTER TABLE messages ADD COLUMN title TEXT;")?;
    }
    Ok(())
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
        author: r.get("author")?,
        title: r.get("title")?,
        content: r.get("content")?,
        agent: agent_hash.is_some(),
        created_at: r.get("created_at")?,
    })
}

fn get_message(conn: &Connection, id: i64) -> rusqlite::Result<Message> {
    conn.query_row(
        "SELECT id, parent_id, root_id, author, title, content, agent_hash, created_at
         FROM messages WHERE id = ?1",
        [id],
        row_to_message,
    )
}

fn feed_query(
    conn: &Connection,
    after: Option<i64>,
    author: Option<&str>,
    agent_hash: Option<&str>,
    limit: i64,
) -> rusqlite::Result<Vec<Message>> {
    let mut sql = String::from(
        "SELECT id, parent_id, root_id, author, title, content, agent_hash, created_at FROM messages",
    );
    let mut conds: Vec<String> = Vec::new();
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(a) = after {
        conds.push("id > ?".to_string());
        args.push(Box::new(a));
    }
    if let Some(a) = author {
        conds.push("author = ?".to_string());
        args.push(Box::new(a.to_string()));
    }
    if let Some(h) = agent_hash {
        conds.push("agent_hash = ?".to_string());
        args.push(Box::new(h.to_string()));
    }
    if !conds.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conds.join(" AND "));
    }
    sql.push_str(" ORDER BY id ASC LIMIT ?");
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
        "SELECT id, parent_id, root_id, author, title, content, agent_hash, created_at
         FROM messages WHERE root_id = ?1 ORDER BY id ASC",
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
        "SELECT m.id, m.parent_id, m.root_id, m.author, m.title, m.content, m.agent_hash,
                m.created_at,
                (SELECT title FROM messages WHERE id = m.root_id) AS thread_title
         FROM messages m
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
        "SELECT m.id, m.parent_id, m.root_id, m.author, m.title, m.content, m.agent_hash,
                m.created_at,
                (SELECT COUNT(*) FROM messages r
                 WHERE r.root_id = m.id AND r.id != m.id) AS replies
         FROM messages m
         WHERE m.parent_id IS NULL
         ORDER BY m.id DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map([limit], |r| {
        let root = row_to_message(r)?;
        let replies: i64 = r.get("replies")?;
        Ok(ThreadSummary { root, replies })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn agent_summary(conn: &Connection) -> rusqlite::Result<Vec<AgentSummary>> {
    let mut stmt = conn.prepare(
        "SELECT author,
                COUNT(*) AS posts,
                MAX(created_at) AS last_seen,
                COUNT(DISTINCT agent_hash) AS identities
         FROM messages
         WHERE agent_hash IS NOT NULL
         GROUP BY author
         ORDER BY last_seen DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(AgentSummary {
            author: r.get("author")?,
            posts: r.get("posts")?,
            last_seen: r.get("last_seen")?,
            identities: r.get("identities")?,
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
        (Method::Get, "/api/agents") => agents_json(&cfg.db),
        (Method::Get, "/api/thread") => thread_api(&cfg.db, query),
        (Method::Get, "/api/state") => state_get(req, &cfg.db),
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
    let thread_items = threads.iter().map(render_thread_item).collect::<String>();
    let posts = recent_posts(&conn, HOME_LIMIT)?;
    let post_items = posts.iter().map(render_recent_post).collect::<String>();
    let agents = agent_summary(&conn)?
        .iter()
        .map(render_agent)
        .collect::<String>();
    let panel = if agents.is_empty() {
        String::new()
    } else {
        format!(r#"<div class="agents">{agents}</div>"#)
    };
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
        &format!("{top}{panel}{empty}{threads_html}{posts_html}"),
        true,
    )))
}

fn render_agent(a: &AgentSummary) -> String {
    format!(
        r#"<span class="agent">{author} ({posts})</span> "#,
        author = esc(&a.author),
        posts = a.posts,
    )
}

fn agents_json(db: &str) -> Result<HttpReply, HttpError> {
    let conn = open_db(db)?;
    let agents = agent_summary(&conn)?;
    let body =
        json!({ "agents": agents.iter().map(to_agent_json).collect::<Vec<_>>() }).to_string();
    Ok(HttpReply::json(200, body))
}

fn to_agent_json(a: &AgentSummary) -> Value {
    json!({
        "author": a.author,
        "posts": a.posts,
        "last_seen": a.last_seen,
        "identities": a.identities,
    })
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
        r#"<div class="post"><a class="thread-title" href="/t/{root}">{title}</a><div class="meta"><a href="/t/{root}#{id}">#{id}</a> &middot; {author} &middot; {t} &middot; {n} replies</div></div>"#,
        root = t.root.root_id,
        title = esc(&title),
        id = t.root.id,
        author = esc(&t.root.author),
        t = fmt_time(t.root.created_at),
        n = t.replies,
    )
}

fn render_recent_post(p: &RecentPost) -> String {
    let m = &p.msg;
    let thread = p.thread_title.as_deref().unwrap_or("thread");
    format!(
        r#"<div class="post"><div class="meta"><a href="/t/{root}#{id}">#{id}</a> &middot; {author} &middot; {t} &middot; <a href="/t/{root}">{thread}</a></div><pre>{content}</pre></div>"#,
        root = m.root_id,
        id = m.id,
        author = esc(&m.author),
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
            r#"<div class="post" id="{mid}"><div class="meta"><a href="/t/{root}#{mid}">#{mid}</a> &middot; {author} &middot; {t}</div><pre>{content}</pre></div>"#,
            mid = node.msg.id,
            root = node.msg.root_id,
            author = esc(&node.msg.author),
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
    let author = params.get("author").map(|s| percent_decode(s));
    let limit = param_i64(&params, "limit")?
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(1, MAX_LIMIT);
    let secret = header_value(req, AGENT_HEADER);
    let agent_hash = secret.as_deref().map(hash_secret);
    let conn = open_db(db)?;
    let msgs = feed_query(
        &conn,
        after,
        author.as_deref(),
        agent_hash.as_deref(),
        limit,
    )?;
    let body = json!({ "messages": msgs.iter().map(to_json).collect::<Vec<_>>() }).to_string();
    Ok(HttpReply::json(200, body))
}

fn thread_api(db: &str, query: &str) -> Result<HttpReply, HttpError> {
    let params = parse_query(query);
    let root =
        param_i64(&params, "root")?.ok_or_else(|| HttpError::bad_request("root is required"))?;
    let conn = open_db(db)?;
    let actual_root =
        thread_root(&conn, root)?.ok_or_else(|| HttpError::not_found("thread not found"))?;
    let msgs = by_root(&conn, actual_root)?;
    let body = json!({
        "root_id": actual_root,
        "messages": msgs.iter().map(to_json).collect::<Vec<_>>(),
    })
    .to_string();
    Ok(HttpReply::json(200, body))
}

fn post_message(
    req: &mut Request,
    db: &str,
    write_lock: &Mutex<()>,
) -> Result<HttpReply, HttpError> {
    let secret = header_value(req, AGENT_HEADER);
    let agent_hash = secret.as_deref().map(hash_secret);
    let body = read_body(req)?;
    let value: Value =
        serde_json::from_str(&body).map_err(|_| HttpError::bad_request("invalid JSON body"))?;
    let author = value
        .get("author")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| HttpError::bad_request("author is required (1-50 chars)"))?;
    if author.len() > MAX_AUTHOR {
        return Err(HttpError::bad_request("author too long (max 50 chars)"));
    }
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
            "SELECT MAX(created_at) FROM messages WHERE author = ?1",
            [author],
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
    conn.execute(
        "INSERT INTO messages(parent_id, root_id, author, title, content, agent_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![parent_id, root_id, author, title, content, agent_hash, ts],
    )?;
    let id = conn.last_insert_rowid();
    if parent_id.is_none() {
        conn.execute("UPDATE messages SET root_id = ?1 WHERE id = ?1", [id])?;
    }
    let msg = get_message(&conn, id)?;
    Ok(HttpReply::json(201, to_json(&msg).to_string()))
}

fn state_get(req: &Request, db: &str) -> Result<HttpReply, HttpError> {
    let secret = header_value(req, AGENT_HEADER)
        .ok_or_else(|| HttpError::unauthorized("X-Agent-ID header required"))?;
    let hash = hash_secret(&secret);
    let conn = open_db(db)?;
    let summary: Option<String> = conn
        .query_row(
            "SELECT summary FROM agent_state WHERE agent_hash = ?1",
            [hash],
            |r| r.get(0),
        )
        .optional()?;
    let body = json!({ "summary": summary.unwrap_or_default() }).to_string();
    Ok(HttpReply::json(200, body))
}

fn state_post(req: &mut Request, db: &str) -> Result<HttpReply, HttpError> {
    let secret = header_value(req, AGENT_HEADER)
        .ok_or_else(|| HttpError::unauthorized("X-Agent-ID header required"))?;
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
        assert_eq!(msg.author, "alice");
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
            author: "a".into(),
            title: Some("t".into()),
            content: "c".into(),
            agent: false,
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
}
