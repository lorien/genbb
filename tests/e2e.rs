use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use genbb::{BoardServer, hash_secret};

static DB_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_db() -> String {
    let n = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir()
        .join(format!("genbb-e2e-{}-{}.db", std::process::id(), n))
        .to_string_lossy()
        .into_owned()
}

struct TestServer {
    server: BoardServer,
    db_path: String,
}

impl TestServer {
    fn start() -> Self {
        let db = temp_db();
        let server = BoardServer::start("127.0.0.1", 0, &db, 2).unwrap();
        Self {
            server,
            db_path: db,
        }
    }

    fn addr(&self) -> String {
        self.server.addr().to_string()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_file(format!("{}-wal", self.db_path));
        let _ = std::fs::remove_file(format!("{}-shm", self.db_path));
    }
}

struct HttpResp {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

fn http(
    addr: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> HttpResp {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    if let Some(b) = body {
        req.push_str(&format!("Content-Length: {}\r\n", b.len()));
    }
    req.push_str("Connection: close\r\n\r\n");
    if let Some(b) = body {
        req.push_str(b);
    }
    stream.write_all(req.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let mut parts = raw.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("");
    let resp_body = parts.next().unwrap_or("").to_string();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    HttpResp {
        status,
        headers,
        body: resp_body,
    }
}

fn post_json(addr: &str, body: &str, agent: Option<&str>) -> HttpResp {
    let mut headers: Vec<(&str, &str)> = vec![("Content-Type", "application/json")];
    if let Some(s) = agent {
        headers.push(("X-Agent-ID", s));
    }
    http(addr, "POST", "/api/messages", &headers, Some(body))
}

fn body_json(resp: &HttpResp) -> serde_json::Value {
    serde_json::from_str(&resp.body).unwrap_or(serde_json::Value::Null)
}

fn msgs(resp: &HttpResp) -> Vec<serde_json::Value> {
    body_json(resp)["messages"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn summary(resp: &HttpResp) -> String {
    body_json(resp)["summary"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn post_top_level_and_reply() {
    let s = TestServer::start();
    let a = s.addr();
    let top = post_json(&a, r#"{"author":"alice","content":"hello board"}"#, None);
    assert_eq!(top.status, 201);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    assert_eq!(body_json(&top)["root_id"].as_i64(), Some(top_id));

    let reply = post_json(
        &a,
        &format!(r#"{{"author":"bob","content":"hi alice","parent_id":{top_id}}}"#),
        None,
    );
    assert_eq!(reply.status, 201);
    let reply_id = body_json(&reply)["id"].as_i64().unwrap();
    assert_eq!(body_json(&reply)["parent_id"].as_i64(), Some(top_id));
    assert_eq!(body_json(&reply)["root_id"].as_i64(), Some(top_id));

    let feed = http(&a, "GET", "/api/messages", &[], None);
    assert_eq!(feed.status, 200);
    let list = msgs(&feed);
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"].as_i64(), Some(top_id));
    assert_eq!(list[1]["id"].as_i64(), Some(reply_id));
}

#[test]
fn feed_filters_after_author_limit() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"author":"ann","content":"one"}"#, None);
    post_json(&a, r#"{"author":"ben","content":"two"}"#, None);
    let three = post_json(&a, r#"{"author":"cat","content":"three"}"#, None);
    let third_id = body_json(&three)["id"].as_i64().unwrap();

    let author_filter = http(&a, "GET", "/api/messages?author=ann", &[], None);
    let list = msgs(&author_filter);
    assert_eq!(list.len(), 1);
    assert!(list.iter().all(|m| m["author"] == "ann"));

    let after = http(
        &a,
        "GET",
        &format!("/api/messages?after={third_id}"),
        &[],
        None,
    );
    assert!(msgs(&after).is_empty());

    let limit = http(&a, "GET", "/api/messages?limit=1", &[], None);
    assert_eq!(msgs(&limit).len(), 1);
}

#[test]
fn agent_posts_fetched_by_header() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"author":"someone","content":"no id"}"#, None);
    post_json(
        &a,
        r#"{"author":"agent-x","content":"mine"}"#,
        Some("secret-abc"),
    );

    let mine = http(
        &a,
        "GET",
        "/api/messages",
        &[("X-Agent-ID", "secret-abc")],
        None,
    );
    let list = msgs(&mine);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["author"], "agent-x");

    let wrong = http(
        &a,
        "GET",
        "/api/messages",
        &[("X-Agent-ID", "other-secret")],
        None,
    );
    assert!(msgs(&wrong).is_empty());
}

#[test]
fn state_requires_header_and_roundtrips() {
    let s = TestServer::start();
    let a = s.addr();

    let no_header = http(&a, "GET", "/api/state", &[], None);
    assert_eq!(no_header.status, 401);

    let empty = http(&a, "GET", "/api/state", &[("X-Agent-ID", "s1")], None);
    assert_eq!(empty.status, 200);
    assert_eq!(summary(&empty), "");

    let write = http(
        &a,
        "POST",
        "/api/state",
        &[("X-Agent-ID", "s1"), ("Content-Type", "application/json")],
        Some(r#"{"summary":"remember this"}"#),
    );
    assert_eq!(write.status, 200);

    let read = http(&a, "GET", "/api/state", &[("X-Agent-ID", "s1")], None);
    assert_eq!(summary(&read), "remember this");

    let other = http(&a, "GET", "/api/state", &[("X-Agent-ID", "s2")], None);
    assert_eq!(summary(&other), "");
}

#[test]
fn rate_limit_blocks_same_author() {
    let s = TestServer::start();
    let a = s.addr();
    let first = post_json(&a, r#"{"author":"fast","content":"first"}"#, None);
    assert_eq!(first.status, 201);

    let second = post_json(&a, r#"{"author":"fast","content":"second"}"#, None);
    assert_eq!(second.status, 429);
    assert!(
        second
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("Retry-After") && v.parse::<u64>().is_ok())
    );

    let other = post_json(&a, r#"{"author":"slowpoke","content":"fine"}"#, None);
    assert_eq!(other.status, 201);
}

#[test]
fn validation_errors() {
    let s = TestServer::start();
    let a = s.addr();

    assert_eq!(
        post_json(&a, r#"{"author":"","content":"x"}"#, None).status,
        400
    );
    assert_eq!(post_json(&a, r#"{"author":"x"}"#, None).status, 400);
    assert_eq!(post_json(&a, r#"{"content":"x"}"#, None).status, 400);
    assert_eq!(post_json(&a, r#"not json"#, None).status, 400);
    assert_eq!(
        post_json(&a, r#"{"author":"a","content":"x","parent_id":"no"}"#, None).status,
        400
    );
    assert_eq!(
        post_json(
            &a,
            r#"{"author":"a","content":"x","parent_id":99999}"#,
            None
        )
        .status,
        400
    );

    let long_content = "x".repeat(2001);
    let long = format!(r#"{{"author":"a","content":"{long_content}"}}"#);
    assert_eq!(post_json(&a, &long, None).status, 400);

    let long_author = "a".repeat(41);
    let long = format!(r#"{{"author":"{long_author}","content":"x"}}"#);
    assert_eq!(post_json(&a, &long, None).status, 400);
}

#[test]
fn thread_api_and_html_pages() {
    let s = TestServer::start();
    let a = s.addr();
    let top = post_json(&a, r#"{"author":"t1","content":"root post"}"#, None);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let r1 = post_json(
        &a,
        &format!(r#"{{"author":"t2","content":"reply one","parent_id":{top_id}}}"#),
        None,
    );
    let r1_id = body_json(&r1)["id"].as_i64().unwrap();
    post_json(
        &a,
        &format!(r#"{{"author":"t3","content":"nested","parent_id":{r1_id}}}"#),
        None,
    );

    let thread = http(&a, "GET", &format!("/api/thread?root={top_id}"), &[], None);
    assert_eq!(thread.status, 200);
    assert_eq!(body_json(&thread)["root_id"].as_i64(), Some(top_id));
    assert_eq!(msgs(&thread).len(), 3);

    let missing = http(&a, "GET", "/api/thread?root=99999", &[], None);
    assert_eq!(missing.status, 404);

    let index = http(&a, "GET", "/", &[], None);
    assert_eq!(index.status, 200);
    assert!(index.body.contains("root post"));
    assert!(index.body.contains("<meta http-equiv=\"refresh\""));

    let thread_html = http(&a, "GET", &format!("/t/{top_id}"), &[], None);
    assert_eq!(thread_html.status, 200);
    assert!(thread_html.body.contains("reply one"));
}

#[test]
fn html_escapes_user_content() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(
        &a,
        r#"{"author":"<b>evil</b>","content":"<script>alert(1)</script>"}"#,
        None,
    );
    let index = http(&a, "GET", "/", &[], None);
    assert!(index.body.contains("&lt;script&gt;"));
    assert!(!index.body.contains("<script>alert(1)</script>"));
}

#[test]
fn raw_secret_never_stored() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(
        &a,
        r#"{"author":"spy","content":"top secret"}"#,
        Some("raw-secret-value-xyz"),
    );
    let conn = rusqlite::Connection::open(&s.db_path).unwrap();
    let hashes: Vec<String> = conn
        .prepare("SELECT agent_hash FROM messages WHERE agent_hash IS NOT NULL")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|x| x.unwrap())
        .collect();
    assert_eq!(hashes.len(), 1);
    assert_ne!(hashes[0], "raw-secret-value-xyz");
    assert_eq!(hashes[0], hash_secret("raw-secret-value-xyz"));
    assert_eq!(hashes[0].len(), 64);
}
