use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use genbb::{BoardServer, ROOT_ID, hash_secret};

// Valid 64-hex agent secrets, as `openssl rand -hex 32` prints.
const SECRET_A: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const SECRET_B: &str = "2222222222222222222222222222222222222222222222222222222222222222";
const SECRET_C: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const SECRET_D: &str = "4444444444444444444444444444444444444444444444444444444444444444";
const SECRET_E: &str = "5555555555555555555555555555555555555555555555555555555555555555";
const SECRET_F: &str = "6666666666666666666666666666666666666666666666666666666666666666";
const SECRET_G: &str = "7777777777777777777777777777777777777777777777777777777777777777";
const SECRET_H: &str = "8888888888888888888888888888888888888888888888888888888888888888";
const SECRET_I: &str = "9999999999999999999999999999999999999999999999999999999999999999";

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
    rules_path: String,
    loop_path: String,
    how_to_loop_path: String,
    root_pwd_path: String,
}

impl TestServer {
    fn start() -> Self {
        Self::with_paths(
            &temp_rules(
                "The board is at http://127.0.0.1:8065 by default.\n\
                 test rules: agents fetch /rules and follow it",
            ),
            &temp_script("#!/usr/bin/env bash\nURL=${URL:-http://127.0.0.1:8065}\nopencode run\n"),
            &temp_doc(
                "Running your agent in a loop\n\
                 curl -LO http://127.0.0.1:8065/agent-loop.sh\n\
                 - why loops: the board is pull-only\n",
            ),
            "https://genbb.org",
        )
    }

    fn with_rules(rules_path: &str) -> Self {
        Self::with_paths(
            rules_path,
            &temp_script("#!/usr/bin/env bash\nURL=${URL:-http://127.0.0.1:8065}\n"),
            &temp_doc("doc"),
            "https://genbb.org",
        )
    }

    fn with_paths(
        rules_path: &str,
        loop_path: &str,
        how_to_loop_path: &str,
        public_url: &str,
    ) -> Self {
        let db = temp_db();
        let root_pwd = temp_root_pwd();
        let server = BoardServer::start(
            "127.0.0.1",
            0,
            &db,
            rules_path,
            loop_path,
            how_to_loop_path,
            public_url,
            &root_pwd,
            2,
        )
        .unwrap();
        Self {
            server,
            db_path: db,
            rules_path: rules_path.to_string(),
            loop_path: loop_path.to_string(),
            how_to_loop_path: how_to_loop_path.to_string(),
            root_pwd_path: root_pwd,
        }
    }

    fn addr(&self) -> String {
        self.server.addr().to_string()
    }
}

fn temp_root_pwd() -> String {
    let n = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("genbb-e2e-pwd-{}-{}.pwd", std::process::id(), n));
    path.to_string_lossy().into_owned()
}

fn temp_rules(content: &str) -> String {
    let n = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("genbb-e2e-rules-{}-{}.txt", std::process::id(), n));
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}

fn temp_script(content: &str) -> String {
    let n = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("genbb-e2e-loop-{}-{}.sh", std::process::id(), n));
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}

fn temp_doc(content: &str) -> String {
    let n = DB_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("genbb-e2e-doc-{}-{}.md", std::process::id(), n));
    std::fs::write(&path, content).unwrap();
    path.to_string_lossy().into_owned()
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_file(format!("{}-wal", self.db_path));
        let _ = std::fs::remove_file(format!("{}-shm", self.db_path));
        let _ = std::fs::remove_file(&self.rules_path);
        let _ = std::fs::remove_file(&self.loop_path);
        let _ = std::fs::remove_file(&self.how_to_loop_path);
        let _ = std::fs::remove_file(&self.root_pwd_path);
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

fn post_json(addr: &str, body: &str, secret: &str) -> HttpResp {
    let payload = match serde_json::from_str::<serde_json::Value>(body) {
        Ok(mut v) => {
            if v.get("parent_id").is_none() && v.get("title").is_none() {
                v["title"] = serde_json::json!("test title");
            }
            v.to_string()
        }
        Err(_) => body.to_string(),
    };
    http(
        addr,
        "POST",
        "/api/messages",
        &[("Content-Type", "application/json"), ("X-Agent-ID", secret)],
        Some(&payload),
    )
}

fn dyn_secret(n: u8) -> String {
    format!("{n:02x}").repeat(32)
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
    let top = post_json(&a, r#"{"content":"hello board"}"#, SECRET_A);
    assert_eq!(top.status, 201);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    assert_eq!(body_json(&top)["root_id"].as_i64(), Some(top_id));

    let reply = post_json(
        &a,
        &format!(r#"{{"content":"hi alice","parent_id":{top_id}}}"#),
        SECRET_B,
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
fn feed_filters_after_agent_limit() {
    let s = TestServer::start();
    let a = s.addr();
    let one = post_json(&a, r#"{"content":"one"}"#, SECRET_A);
    let one_agent = body_json(&one)["agent_id"].as_str().unwrap().to_string();
    post_json(&a, r#"{"content":"two"}"#, SECRET_B);
    let three = post_json(&a, r#"{"content":"three"}"#, SECRET_C);
    let third_id = body_json(&three)["id"].as_i64().unwrap();

    let agent_filter = http(
        &a,
        "GET",
        &format!("/api/messages?agent_id={one_agent}"),
        &[],
        None,
    );
    let list = msgs(&agent_filter);
    assert_eq!(list.len(), 1);
    assert!(
        list.iter()
            .all(|m| m["agent_id"].as_str() == Some(one_agent.as_str()))
    );

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
    post_json(&a, r#"{"content":"someone"}"#, SECRET_A);
    post_json(&a, r#"{"content":"mine"}"#, SECRET_B);

    let mine = http(
        &a,
        "GET",
        "/api/messages",
        &[("X-Agent-ID", SECRET_A)],
        None,
    );
    let list = msgs(&mine);
    assert_eq!(list.len(), 1);
    assert!(list[0]["agent_id"].is_string());
    assert!(list[0]["author"].is_null());

    let wrong = http(
        &a,
        "GET",
        "/api/messages",
        &[("X-Agent-ID", SECRET_C)],
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

    let empty = http(&a, "GET", "/api/state", &[("X-Agent-ID", SECRET_C)], None);
    assert_eq!(empty.status, 200);
    assert_eq!(summary(&empty), "");

    let write = http(
        &a,
        "POST",
        "/api/state",
        &[
            ("X-Agent-ID", SECRET_C),
            ("Content-Type", "application/json"),
        ],
        Some(r#"{"summary":"remember this"}"#),
    );
    assert_eq!(write.status, 200);

    let read = http(&a, "GET", "/api/state", &[("X-Agent-ID", SECRET_C)], None);
    assert_eq!(summary(&read), "remember this");

    let other = http(&a, "GET", "/api/state", &[("X-Agent-ID", SECRET_D)], None);
    assert_eq!(summary(&other), "");
}

#[test]
fn rate_limit_blocks_same_identity() {
    let s = TestServer::start();
    let a = s.addr();
    let first = post_json(&a, r#"{"content":"first"}"#, SECRET_A);
    assert_eq!(first.status, 201);

    let second = post_json(&a, r#"{"content":"second"}"#, SECRET_A);
    assert_eq!(second.status, 429);
    assert!(
        second
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("Retry-After") && v.parse::<u64>().is_ok())
    );

    let other = post_json(&a, r#"{"content":"fine"}"#, SECRET_B);
    assert_eq!(other.status, 201);
}

#[test]
fn validation_errors() {
    let s = TestServer::start();
    let a = s.addr();

    // A stray author field is ignored; content is still required.
    assert_eq!(
        post_json(&a, r#"{"author":"bogus","content":"x"}"#, SECRET_A).status,
        201
    );
    assert!(
        body_json(&post_json(
            &a,
            r#"{"author":"bogus","content":"x"}"#,
            SECRET_B
        ))["author"]
            .is_null()
    );
    assert_eq!(post_json(&a, r#"{"title":"t"}"#, SECRET_B).status, 400);
    assert_eq!(post_json(&a, r#"not json"#, SECRET_B).status, 400);
    assert_eq!(
        post_json(&a, r#"{"content":"x","parent_id":"no"}"#, SECRET_B).status,
        400
    );
    assert_eq!(
        post_json(&a, r#"{"content":"x","parent_id":99999}"#, SECRET_C).status,
        400
    );

    let long_content = "x".repeat(2001);
    let long = format!(r#"{{"content":"{long_content}"}}"#);
    assert_eq!(post_json(&a, &long, SECRET_B).status, 400);
}

#[test]
fn headerless_post_rejected() {
    let s = TestServer::start();
    let a = s.addr();
    let resp = http(
        &a,
        "POST",
        "/api/messages",
        &[("Content-Type", "application/json")],
        Some(r#"{"title":"t","content":"x"}"#),
    );
    assert_eq!(resp.status, 401);
}

#[test]
fn thread_api_and_html_pages() {
    let s = TestServer::start();
    let a = s.addr();
    let top = post_json(&a, r#"{"content":"root post"}"#, SECRET_A);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let r1 = post_json(
        &a,
        &format!(r#"{{"content":"reply one","parent_id":{top_id}}}"#),
        SECRET_B,
    );
    let r1_id = body_json(&r1)["id"].as_i64().unwrap();
    post_json(
        &a,
        &format!(r#"{{"content":"nested","parent_id":{r1_id}}}"#),
        SECRET_C,
    );

    let thread = http(&a, "GET", &format!("/api/thread?root={top_id}"), &[], None);
    assert_eq!(thread.status, 200);
    assert_eq!(body_json(&thread)["root_id"].as_i64(), Some(top_id));
    assert_eq!(msgs(&thread).len(), 3);

    let missing = http(&a, "GET", "/api/thread?root=99999", &[], None);
    assert_eq!(missing.status, 404);

    let index = http(&a, "GET", "/", &[], None);
    assert_eq!(index.status, 200);
    assert!(index.body.contains("test title"));
    assert!(index.body.contains(&format!("/t/{top_id}")));
    assert!(index.body.contains("<meta http-equiv=\"refresh\""));
    assert!(index.body.contains(&format!("#{top_id}")));

    let thread_html = http(&a, "GET", &format!("/t/{top_id}"), &[], None);
    assert_eq!(thread_html.status, 200);
    assert!(thread_html.body.contains("reply one"));
}

#[test]
fn html_escapes_user_content() {
    let s = TestServer::start();
    let a = s.addr();
    let top = post_json(
        &a,
        r#"{"title":"<b>evil</b>","content":"<script>alert(1)</script>"}"#,
        SECRET_A,
    );
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let index = http(&a, "GET", "/", &[], None);
    assert!(index.body.contains("&lt;b&gt;evil&lt;/b&gt;"));
    let thread_html = http(&a, "GET", &format!("/t/{top_id}"), &[], None);
    assert!(thread_html.body.contains("&lt;script&gt;"));
    assert!(!thread_html.body.contains("<script>alert(1)</script>"));
}

#[test]
fn thread_title_cannot_break_out_of_the_title_tag() {
    let s = TestServer::start();
    let a = s.addr();
    // A title that closes the <title> element and injects script would run in
    // the viewer's browser (root included) on /t/<root>.
    let evil = "x</title><script>alert(1)</script>";
    let top = post_json(
        &a,
        &format!(r#"{{"title":"{evil}","content":"hi"}}"#),
        SECRET_A,
    );
    assert_eq!(top.status, 201);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let thread_html = http(&a, "GET", &format!("/t/{top_id}"), &[], None);
    assert_eq!(thread_html.status, 200);
    assert!(!thread_html.body.contains("</title><script>"));
    assert!(!thread_html.body.contains("<script>alert(1)</script>"));
    assert!(thread_html.body.contains("&lt;/title&gt;&lt;script&gt;"));
}

#[test]
fn raw_secret_never_stored() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"content":"top secret"}"#, SECRET_E);
    let conn = rusqlite::Connection::open(&s.db_path).unwrap();
    let hashes: Vec<String> = conn
        .prepare("SELECT agent_hash FROM messages WHERE agent_hash IS NOT NULL")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|x| x.unwrap())
        .collect();
    assert_eq!(hashes.len(), 1);
    assert_ne!(hashes[0], SECRET_E);
    assert_eq!(hashes[0], hash_secret(SECRET_E));
    assert_eq!(hashes[0].len(), 64);
}

#[test]
fn invalid_query_params_return_400() {
    let s = TestServer::start();
    let a = s.addr();
    assert_eq!(
        http(&a, "GET", "/api/messages?after=abc", &[], None).status,
        400
    );
    assert_eq!(
        http(&a, "GET", "/api/messages?limit=abc", &[], None).status,
        400
    );
    assert_eq!(
        http(&a, "GET", "/api/thread?root=1&excerpt=abc", &[], None).status,
        400
    );
}

#[test]
fn malformed_agent_secret_rejected() {
    let s = TestServer::start();
    let a = s.addr();
    let bad = "not-a-real-secret";

    // Optional header: if sent, it must be well-formed.
    assert_eq!(
        http(&a, "GET", "/api/messages", &[("X-Agent-ID", bad)], None).status,
        400
    );
    assert_eq!(post_json(&a, r#"{"content":"y"}"#, bad).status, 400);

    // Required header: malformed is a 400, missing stays a 401.
    assert_eq!(
        http(&a, "GET", "/api/state", &[("X-Agent-ID", bad)], None).status,
        400
    );
    assert_eq!(
        http(
            &a,
            "POST",
            "/api/state",
            &[("X-Agent-ID", bad), ("Content-Type", "application/json")],
            Some(r#"{"summary":"x"}"#),
        )
        .status,
        400
    );

    // A correct-length secret with non-hex characters is also rejected.
    assert_eq!(
        http(
            &a,
            "GET",
            "/api/messages",
            &[("X-Agent-ID", "z".repeat(64).as_str())],
            None
        )
        .status,
        400
    );
}

#[test]
fn limit_clamped_and_unknown_route() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"content":"one"}"#, SECRET_A);
    post_json(&a, r#"{"content":"two"}"#, SECRET_B);

    let zero = http(&a, "GET", "/api/messages?limit=0", &[], None);
    assert_eq!(msgs(&zero).len(), 1);
    let big = http(&a, "GET", "/api/messages?limit=999", &[], None);
    assert_eq!(msgs(&big).len(), 2);

    assert_eq!(http(&a, "GET", "/nope", &[], None).status, 404);
    assert_eq!(http(&a, "GET", "/t/abc", &[], None).status, 404);
    assert_eq!(
        http(&a, "GET", "/api/thread?root=99999", &[], None).status,
        404
    );
}

#[test]
fn state_post_requires_header() {
    let s = TestServer::start();
    let a = s.addr();
    let resp = http(
        &a,
        "POST",
        "/api/state",
        &[("Content-Type", "application/json")],
        Some(r#"{"summary":"x"}"#),
    );
    assert_eq!(resp.status, 401);
}

#[test]
fn size_caps() {
    let s = TestServer::start();
    let a = s.addr();
    let long_summary = format!(r#"{{"summary":"{}"}}"#, "s".repeat(10001));
    let resp = http(
        &a,
        "POST",
        "/api/state",
        &[
            ("X-Agent-ID", SECRET_F),
            ("Content-Type", "application/json"),
        ],
        Some(&long_summary),
    );
    assert_eq!(resp.status, 400);

    let huge_body = "x".repeat(70000);
    let resp = http(
        &a,
        "POST",
        "/api/messages",
        &[
            ("Content-Type", "application/json"),
            ("X-Agent-ID", SECRET_F),
        ],
        Some(&huge_body),
    );
    assert_eq!(resp.status, 400);
}

#[test]
fn boundary_values_accepted() {
    let s = TestServer::start();
    let a = s.addr();
    let content2000 = "b".repeat(2000);
    let r1 = post_json(&a, &format!(r#"{{"content":"{content2000}"}}"#), SECRET_A);
    assert_eq!(r1.status, 201);
}

#[test]
fn agent_id_feed_filter() {
    let s = TestServer::start();
    let a = s.addr();
    let r = post_json(&a, r#"{"content":"bonjour"}"#, SECRET_A);
    let id = body_json(&r)["agent_id"].as_str().unwrap().to_string();
    let resp = http(
        &a,
        "GET",
        &format!("/api/messages?agent_id={id}"),
        &[],
        None,
    );
    assert_eq!(msgs(&resp).len(), 1);
    let none = http(&a, "GET", "/api/messages?agent_id=deadbeefdead", &[], None);
    assert!(msgs(&none).is_empty());
}

#[test]
fn agent_loop_script_served() {
    let s = TestServer::start();
    let a = s.addr();
    let resp = http(&a, "GET", "/agent-loop.sh", &[], None);
    assert_eq!(resp.status, 200);
    assert!(
        resp.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.contains("x-shellscript"))
    );
    assert!(resp.body.contains("#!/usr/bin/env bash"));
    assert!(resp.body.contains("opencode run"));
    assert!(resp.body.contains("URL=${URL:-https://genbb.org}"));
    assert!(!resp.body.contains("URL=${URL:-http://127.0.0.1:8065}"));
}

#[test]
fn agent_loop_script_missing_returns_404() {
    let missing = std::env::temp_dir().join(format!("genbb-e2e-noloop-{}.sh", std::process::id()));
    let s = TestServer::with_paths(
        &temp_rules("rules"),
        &missing.to_string_lossy(),
        &temp_doc("doc"),
        "https://genbb.org",
    );
    let a = s.addr();
    assert_eq!(http(&a, "GET", "/agent-loop.sh", &[], None).status, 404);
}

#[test]
fn how_to_loop_doc_served() {
    let s = TestServer::start();
    let a = s.addr();
    let resp = http(&a, "GET", "/how-to-loop", &[], None);
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("Running your agent in a loop"));
    assert!(resp.body.contains("pull-only"));
    assert!(resp.body.contains("https://genbb.org/agent-loop.sh"));
    assert!(!resp.body.contains("http://127.0.0.1:8065"));
    assert!(
        resp.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.contains("text/markdown"))
    );
}

#[test]
fn run_github_action_agent_doc_served() {
    let s = TestServer::start();
    let a = s.addr();
    let resp = http(&a, "GET", "/run-github-action-agent", &[], None);
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("fork"));
    assert!(resp.body.contains("GENBB_AGENTS"));
    assert!(
        resp.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.contains("text/markdown"))
    );
}

#[test]
fn how_to_loop_doc_missing_returns_404() {
    let missing = std::env::temp_dir().join(format!("genbb-e2e-nodoc-{}.md", std::process::id()));
    let s = TestServer::with_paths(
        &temp_rules("rules"),
        &temp_script("#!/usr/bin/env bash\n"),
        &missing.to_string_lossy(),
        "https://genbb.org",
    );
    let a = s.addr();
    assert_eq!(http(&a, "GET", "/how-to-loop", &[], None).status, 404);
}

#[test]
fn rules_endpoint_serves_prompt() {
    let s = TestServer::start();
    let a = s.addr();
    let resp = http(&a, "GET", "/rules", &[], None);
    assert_eq!(resp.status, 200);
    assert!(
        resp.body
            .contains("test rules: agents fetch /rules and follow it")
    );
    assert!(resp.body.contains("https://genbb.org"));
    assert!(!resp.body.contains("http://127.0.0.1:8065"));
    assert!(
        resp.headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v.contains("text/plain"))
    );
}

#[test]
fn rules_missing_returns_404() {
    let missing =
        std::env::temp_dir().join(format!("genbb-e2e-norules-{}.txt", std::process::id()));
    let s = TestServer::with_rules(&missing.to_string_lossy());
    let a = s.addr();
    assert_eq!(http(&a, "GET", "/rules", &[], None).status, 404);
}

#[test]
fn index_tells_agents_about_rules() {
    let s = TestServer::start();
    let a = s.addr();
    let resp = http(&a, "GET", "/", &[], None);
    assert_eq!(resp.status, 200);
    assert!(resp.body.contains("/rules"));
    assert!(resp.body.contains("AGENT"));
    assert!(resp.body.contains("/how-to-loop"));
    assert!(resp.body.contains("<b>Users:</b>"));
}

#[test]
fn agents_listing_and_home() {
    let s = TestServer::start();
    let a = s.addr();

    let empty = http(&a, "GET", "/api/agents", &[], None);
    assert_eq!(empty.status, 200);
    assert_eq!(body_json(&empty)["agents"].as_array().unwrap().len(), 0);

    post_json(&a, r#"{"content":"hi"}"#, SECRET_G);
    post_json(&a, r#"{"content":"me too"}"#, SECRET_H);
    post_json(&a, r#"{"content":"yo"}"#, SECRET_I);

    let resp = http(&a, "GET", "/api/agents", &[], None);
    assert_eq!(resp.status, 200);
    let body = body_json(&resp);
    let agents = body["agents"].as_array().unwrap();
    // Three identities -> three entries with distinct 12-hex agent_ids,
    // no name field, no secret leakage.
    assert_eq!(agents.len(), 3);
    assert!(!resp.body.contains(SECRET_G));
    assert!(!resp.body.contains(SECRET_I));
    let mut ids: Vec<&str> = Vec::new();
    for agent in agents {
        assert_eq!(agent["posts"], 1);
        let id = agent["agent_id"].as_str().unwrap();
        assert_eq!(id.len(), 12);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(agent["author"].is_null());
        assert!(agent["last_seen"].is_i64());
        ids.push(id);
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3);
}

#[test]
fn agent_id_is_permanent_and_stable() {
    let s = TestServer::start();
    let a = s.addr();
    let one = post_json(&a, r#"{"content":"one"}"#, SECRET_A);
    let id_a = body_json(&one)["agent_id"].as_str().unwrap().to_string();
    std::thread::sleep(Duration::from_secs(6));
    post_json(&a, r#"{"content":"two"}"#, SECRET_A);
    let mine = post_json(&a, r#"{"content":"mine"}"#, SECRET_B);
    let id_b = body_json(&mine)["agent_id"].as_str().unwrap().to_string();

    assert_eq!(id_a.len(), 12);
    assert_eq!(id_b.len(), 12);
    assert!(id_a.chars().all(|c| c.is_ascii_hexdigit()));
    assert_ne!(id_a, id_b);

    // Same secret, different posts -> same permanent id. No names anywhere.
    let feed = http(&a, "GET", "/api/messages", &[], None);
    let list = msgs(&feed);
    assert_eq!(list.len(), 3);
    assert!(list.iter().all(|m| m["author"].is_null()));
    assert!(list.iter().all(|m| m["agent_id"].is_string()));
    let two = list.iter().find(|m| m["content"] == "two").unwrap();
    assert_eq!(two["agent_id"].as_str(), Some(id_a.as_str()));

    // /api/state reveals the caller's own id.
    let st = http(&a, "GET", "/api/state", &[("X-Agent-ID", SECRET_A)], None);
    assert_eq!(body_json(&st)["agent_id"].as_str(), Some(id_a.as_str()));

    // The agent listing agrees.
    let body = body_json(&http(&a, "GET", "/api/agents", &[], None));
    let arr = body["agents"].as_array().unwrap();
    assert!(
        arr.iter()
            .any(|x| x["agent_id"].as_str() == Some(id_a.as_str()))
    );
}

#[test]
fn title_required_on_top_level() {
    let s = TestServer::start();
    let a = s.addr();
    let no_title = http(
        &a,
        "POST",
        "/api/messages",
        &[
            ("Content-Type", "application/json"),
            ("X-Agent-ID", SECRET_A),
        ],
        Some(r#"{"content":"hi"}"#),
    );
    assert_eq!(no_title.status, 400);

    let long_title = "t".repeat(121);
    let long = format!(r#"{{"title":"{long_title}","content":"hi"}}"#);
    let resp = http(
        &a,
        "POST",
        "/api/messages",
        &[
            ("Content-Type", "application/json"),
            ("X-Agent-ID", SECRET_B),
        ],
        Some(&long),
    );
    assert_eq!(resp.status, 400);

    let ok = post_json(&a, r#"{"title":"my thread","content":"hi"}"#, SECRET_C);
    assert_eq!(ok.status, 201);
    let id = body_json(&ok)["id"].as_i64().unwrap();
    assert_eq!(body_json(&ok)["title"], "my thread");

    let reply = post_json(
        &a,
        &format!(r#"{{"content":"reply","parent_id":{id}}}"#),
        SECRET_D,
    );
    assert_eq!(reply.status, 201);

    let with_title = format!(r#"{{"title":"no","content":"reply","parent_id":{id}}}"#);
    let resp = http(
        &a,
        "POST",
        "/api/messages",
        &[
            ("Content-Type", "application/json"),
            ("X-Agent-ID", SECRET_E),
        ],
        Some(&with_title),
    );
    assert_eq!(resp.status, 400);
}

#[test]
fn title_in_feed_thread_and_home_list() {
    let s = TestServer::start();
    let a = s.addr();
    let top = post_json(&a, r#"{"title":"alpha thread","content":"root"}"#, SECRET_A);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let r1 = post_json(
        &a,
        &format!(r#"{{"content":"reply","parent_id":{top_id}}}"#),
        SECRET_B,
    );
    let r1_id = body_json(&r1)["id"].as_i64().unwrap();
    post_json(
        &a,
        &format!(r#"{{"content":"two","parent_id":{r1_id}}}"#),
        SECRET_C,
    );
    post_json(
        &a,
        r#"{"title":"second thread","content":"other root"}"#,
        SECRET_D,
    );

    let feed = http(&a, "GET", "/api/messages", &[], None);
    assert_eq!(body_json(&feed)["messages"][0]["title"], "alpha thread");
    assert_eq!(
        body_json(&feed)["messages"][1]["title"],
        serde_json::Value::Null
    );

    let thread = http(&a, "GET", &format!("/api/thread?root={top_id}"), &[], None);
    assert_eq!(body_json(&thread)["messages"][0]["title"], "alpha thread");

    let home = http(&a, "GET", "/", &[], None);
    assert!(home.body.contains("alpha thread"));
    assert!(home.body.contains("second thread"));
    assert!(home.body.contains(&format!("#{top_id}")));
    assert!(home.body.contains(&format!("/t/{top_id}#{top_id}")));
    assert!(home.body.contains(&format!("#{r1_id}")));
    assert!(home.body.contains(">alpha thread</a>"));
    assert!(home.body.contains(" \u{2022} "));
    assert!(!home.body.contains(">thread</a>"));
    let thread_html = http(&a, "GET", &format!("/t/{top_id}"), &[], None);
    assert!(thread_html.body.contains("<h1>alpha thread</h1>"));
    assert!(thread_html.body.contains("href=\"/\""));
    assert!(thread_html.body.contains(&format!("id=\"{top_id}\"")));
    assert!(thread_html.body.contains(&format!("/t/{top_id}#{r1_id}")));
    assert!(thread_html.body.contains("</pre></div><div class=\"post\""));
    assert!(!thread_html.body.contains("margin-left"));
}

#[test]
fn home_shows_ten_threads_and_ten_posts() {
    let s = TestServer::start();
    let a = s.addr();
    for i in 0..12 {
        post_json(
            &a,
            &format!(r#"{{"title":"thread {i}","content":"root {i}"}}"#),
            &dyn_secret(i),
        );
    }
    let top = post_json(
        &a,
        r#"{"title":"head","content":"top root"}"#,
        &dyn_secret(20),
    );
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let r1 = post_json(
        &a,
        &format!(r#"{{"content":"reply body","parent_id":{top_id}}}"#),
        &dyn_secret(21),
    );
    let r1_id = body_json(&r1)["id"].as_i64().unwrap();

    let home = http(&a, "GET", "/", &[], None);
    assert_eq!(home.status, 200);
    assert!(home.body.contains("<h2>Recent threads</h2>"));
    assert!(home.body.contains("<h2>Recent posts</h2>"));
    let threads = home.body.matches(r#"class="thread-title""#).count();
    assert_eq!(threads, 10);
    assert!(!home.body.contains("thread 2"));
    assert!(home.body.contains("thread 3"));
    assert!(home.body.contains("reply body"));
    assert!(home.body.contains(&format!("#{r1_id}")));
    assert!(home.body.contains(&format!("/t/{top_id}#{r1_id}")));
}

#[test]
fn head_reports_latest_id_and_counts() {
    let s = TestServer::start();
    let a = s.addr();

    let empty = http(&a, "GET", "/api/head", &[], None);
    assert_eq!(empty.status, 200);
    let b = body_json(&empty);
    assert_eq!(b["latest_id"], 0);
    assert_eq!(b["messages"], 0);
    assert_eq!(b["agents"], 0);

    let top = post_json(&a, r#"{"content":"one"}"#, SECRET_A);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    post_json(
        &a,
        &format!(r#"{{"content":"two","parent_id":{top_id}}}"#),
        SECRET_B,
    );

    let resp = http(&a, "GET", "/api/head", &[], None);
    assert_eq!(resp.status, 200);
    let b = body_json(&resp);
    assert_eq!(b["latest_id"], top_id + 1);
    assert_eq!(b["messages"], 2);
    assert_eq!(b["agents"], 2);
}

#[test]
fn feed_excerpt_truncates_at_word_boundary() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"content":"alpha beta gamma"}"#, SECRET_A);

    // Cut at index 6 is mid "beta" -> back off to the space: "alpha".
    let cut = http(&a, "GET", "/api/messages?excerpt=6", &[], None);
    let m = &msgs(&cut)[0];
    assert_eq!(m["content"], "alpha");
    assert_eq!(m["truncated"], true);

    // Cut at index 5 lands exactly on a space: still "alpha".
    let cut = http(&a, "GET", "/api/messages?excerpt=5", &[], None);
    assert_eq!(msgs(&cut)[0]["content"], "alpha");

    // Excerpt larger than the content: full text, no flag.
    let big = http(&a, "GET", "/api/messages?excerpt=200", &[], None);
    let m = &msgs(&big)[0];
    assert_eq!(m["content"], "alpha beta gamma");
    assert!(m.get("truncated").is_none());

    // One unbroken word longer than the excerpt -> empty content, flagged.
    post_json(&a, r#"{"content":"supercalifragilistic"}"#, SECRET_B);
    let no_ws = http(&a, "GET", "/api/messages?excerpt=5", &[], None);
    let list = msgs(&no_ws);
    let m = list
        .iter()
        .find(|m| m["content"].as_str() == Some(""))
        .expect("unbroken-word post should be empty");
    assert_eq!(m["truncated"], true);
}

#[test]
fn thread_excerpt_truncates() {
    let s = TestServer::start();
    let a = s.addr();
    let top = post_json(&a, r#"{"content":"root alpha beta"}"#, SECRET_A);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    post_json(
        &a,
        &format!(r#"{{"content":"reply one two three","parent_id":{top_id}}}"#),
        SECRET_B,
    );

    let full = http(&a, "GET", &format!("/api/thread?root={top_id}"), &[], None);
    assert_eq!(msgs(&full)[0]["content"], "root alpha beta");

    let cut = http(
        &a,
        "GET",
        &format!("/api/thread?root={top_id}&excerpt=6"),
        &[],
        None,
    );
    assert_eq!(msgs(&cut)[0]["content"], "root");
    assert_eq!(msgs(&cut)[0]["truncated"], true);
}

#[test]
fn excerpt_invalid_values_rejected() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"content":"one"}"#, SECRET_A);

    assert_eq!(
        http(&a, "GET", "/api/messages?excerpt=abc", &[], None).status,
        400
    );
    assert_eq!(
        http(&a, "GET", "/api/messages?excerpt=0", &[], None).status,
        400
    );
    assert_eq!(
        http(&a, "GET", "/api/messages?excerpt=2001", &[], None).status,
        400
    );
    assert_eq!(
        http(&a, "GET", "/api/thread?root=1&excerpt=abc", &[], None).status,
        400
    );
}

#[test]
fn message_json_has_no_agent_field() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"content":"one"}"#, SECRET_A);

    let feed = http(&a, "GET", "/api/messages", &[], None);
    let list = msgs(&feed);
    assert!(list.iter().all(|m| m.get("agent").is_none()));
    // Fields we still carry.
    let m = &list[0];
    for key in [
        "id",
        "parent_id",
        "root_id",
        "title",
        "content",
        "agent_id",
        "created_at",
    ] {
        assert!(m.get(key).is_some(), "missing {key}");
    }
}

#[test]
fn mentions_filters_to_agents_threads() {
    let s = TestServer::start();
    let a = s.addr();

    // A starts a thread; B replies inside it. C starts an unrelated thread.
    let top = post_json(&a, r#"{"content":"root alpha"}"#, SECRET_A);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let b_reply = post_json(
        &a,
        &format!(r#"{{"content":"reply from b","parent_id":{top_id}}}"#),
        SECRET_B,
    );
    let b_agent = body_json(&b_reply)["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    post_json(&a, r#"{"content":"root beta"}"#, SECRET_C);

    // Mentions of B = every message in a thread B posted in: the A-B thread,
    // both posts, and nothing from C's thread.
    let resp = http(
        &a,
        "GET",
        &format!("/api/messages?mentions={b_agent}"),
        &[],
        None,
    );
    assert_eq!(resp.status, 200);
    let list = msgs(&resp);
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|m| m["root_id"] == top_id));
    assert!(list.iter().all(|m| {
        m["content"]
            .as_str()
            .is_some_and(|c| c.contains("alpha") || c.contains("from b"))
    }));
}

#[test]
fn mentions_unknown_agent_empty() {
    let s = TestServer::start();
    let a = s.addr();
    post_json(&a, r#"{"content":"one"}"#, SECRET_A);

    let resp = http(&a, "GET", "/api/messages?mentions=deadbeefdead", &[], None);
    assert_eq!(resp.status, 200);
    assert!(msgs(&resp).is_empty());
}

#[test]
fn mentions_composes_with_after_and_excerpt() {
    let s = TestServer::start();
    let a = s.addr();

    let top = post_json(&a, r#"{"content":"alpha beta gamma"}"#, SECRET_A);
    let top_id = body_json(&top)["id"].as_i64().unwrap();
    let b_reply = post_json(
        &a,
        &format!(r#"{{"content":"reply delta","parent_id":{top_id}}}"#),
        SECRET_B,
    );
    let b_agent = body_json(&b_reply)["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let reply_id = body_json(&b_reply)["id"].as_i64().unwrap();

    // after=<root> keeps only B's reply within her thread.
    let resp = http(
        &a,
        "GET",
        &format!("/api/messages?mentions={b_agent}&after={top_id}"),
        &[],
        None,
    );
    let list = msgs(&resp);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], reply_id);

    // excerpt applies to the mentions feed.
    let resp = http(
        &a,
        "GET",
        &format!("/api/messages?mentions={b_agent}&excerpt=5"),
        &[],
        None,
    );
    let list = msgs(&resp);
    assert!(list.iter().all(|m| m.get("truncated").is_some()));
}

#[test]
fn session_endpoint_returns_snapshot() {
    let s = TestServer::start();
    let a = s.addr();

    let one = post_json(&a, r#"{"content":"my post"}"#, SECRET_A);
    let my_id = body_json(&one)["agent_id"].as_str().unwrap().to_string();
    let post_id = body_json(&one)["id"].as_i64().unwrap();
    // Another agent posts too, so counts differ from "my posts".
    post_json(&a, r#"{"content":"other"}"#, SECRET_B);
    http(
        &a,
        "POST",
        "/api/state",
        &[
            ("X-Agent-ID", SECRET_A),
            ("Content-Type", "application/json"),
        ],
        Some(r#"{"summary":"talking to bob"}"#),
    );

    let resp = http(&a, "GET", "/api/session", &[("X-Agent-ID", SECRET_A)], None);
    assert_eq!(resp.status, 200);
    let b = body_json(&resp);
    assert_eq!(b["summary"], "talking to bob");
    assert_eq!(b["agent_id"], my_id);
    assert_eq!(b["my_messages"].as_array().unwrap().len(), 1);
    assert_eq!(b["latest_id"], post_id + 1);
    assert_eq!(b["messages"], 2);
    let agents = b["agents"].as_array().unwrap();
    assert!(agents.iter().any(|x| x["agent_id"] == my_id));
}

#[test]
fn session_requires_header() {
    let s = TestServer::start();
    let a = s.addr();
    assert_eq!(http(&a, "GET", "/api/session", &[], None).status, 401);
}

// ---- root user sessions ----

const PWD_SALT: &str = "a1b2c3d4e5f60718a1b2c3d4e5f60718";
const ROOT_PASSWORD: &str = "root-secret";

fn encode_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn form_post(addr: &str, path: &str, fields: &[(&str, &str)], cookie: Option<&str>) -> HttpResp {
    let body: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
        .collect();
    let body = body.join("&");
    let mut headers: Vec<(&str, &str)> =
        vec![("Content-Type", "application/x-www-form-urlencoded")];
    if let Some(c) = cookie {
        headers.push(("Cookie", c));
    }
    http(addr, "POST", path, &headers, Some(&body))
}

fn location(resp: &HttpResp) -> Option<String> {
    resp.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("location"))
        .map(|(_, v)| v.clone())
}

fn set_cookie(resp: &HttpResp) -> Option<String> {
    resp.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("set-cookie"))
        .map(|(_, v)| v.clone())
}

fn cookie_value(sc: &str) -> String {
    sc.split(';')
        .next()
        .and_then(|p| p.split_once('='))
        .map(|(_, v)| v.to_string())
        .unwrap_or_default()
}

/// The full `Cookie` header value the browser would send for a session token.
fn session_cookie(token: &str) -> String {
    format!("genbb_session={token}")
}

/// Write a bootstrap password file exactly as the documented command line does:
/// `{salt}:{sha256(salt:password)}`.
fn write_root_pwd(path: &str, salt: &str, password: &str) {
    let hash = hash_secret(&format!("{salt}:{password}"));
    std::fs::write(path, format!("{salt}:{hash}\n")).unwrap();
}

#[test]
fn login_page_reports_missing_password_file() {
    let s = TestServer::start();
    let a = s.addr();
    // No root record and no password file: the GET form explains that.
    let get = http(&a, "GET", "/user/login", &[], None);
    assert_eq!(get.status, 200);
    assert!(get.body.contains("is missing"), "body: {}", get.body);
    // A login attempt is answered the same way.
    let post = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", "x")],
        None,
    );
    assert_eq!(post.status, 503);
    assert!(post.body.contains("is missing"));
}

#[test]
fn login_rejects_wrong_credentials_uniformly() {
    let s = TestServer::start();
    write_root_pwd(&s.root_pwd_path, PWD_SALT, ROOT_PASSWORD);
    let a = s.addr();
    let wrong_pw = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", "nope")],
        None,
    );
    assert_eq!(wrong_pw.status, 401);
    assert!(wrong_pw.body.contains("invalid login or password"));
    // An unknown login gets the exact same error (no user enumeration).
    let wrong_user = form_post(
        &a,
        "/user/login",
        &[("login", "alice"), ("password", ROOT_PASSWORD)],
        None,
    );
    assert_eq!(wrong_user.status, 401);
    assert!(wrong_user.body.contains("invalid login or password"));
    // Nothing was created on failure: the bootstrap file is still there.
    assert!(std::path::Path::new(&s.root_pwd_path).exists());
}

#[test]
fn first_login_migrates_password_file_and_starts_session() {
    let s = TestServer::start();
    write_root_pwd(&s.root_pwd_path, PWD_SALT, ROOT_PASSWORD);
    let a = s.addr();

    let login = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", ROOT_PASSWORD)],
        None,
    );
    assert_eq!(login.status, 303);
    assert_eq!(location(&login).as_deref(), Some("/"));
    let sc = set_cookie(&login).expect("set-cookie header");
    assert!(sc.contains("genbb_session="), "cookie: {sc}");
    assert!(sc.contains("HttpOnly"));
    assert!(sc.contains("SameSite=Lax"));
    assert!(sc.contains("Path=/"));
    let token = cookie_value(&sc);
    assert!(!token.is_empty());

    // The bootstrap file is gone and the credential lives in the database
    // under a fresh random salt (not the one we wrote).
    assert!(!std::path::Path::new(&s.root_pwd_path).exists());
    let conn = rusqlite::Connection::open(&s.db_path).unwrap();
    let (db_salt, db_hash): (String, String) = conn
        .query_row(
            "SELECT salt, hash FROM users WHERE username = 'root'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_ne!(db_salt, PWD_SALT);
    assert_eq!(hash_secret(&format!("{db_salt}:{ROOT_PASSWORD}")), db_hash);

    // A second login verifies against the database.
    let again = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", ROOT_PASSWORD)],
        None,
    );
    assert_eq!(again.status, 303);

    // Logged in: GET /user/login bounces to home.
    let authed = http(
        &a,
        "GET",
        "/user/login",
        &[("Cookie", &session_cookie(&token))],
        None,
    );
    assert_eq!(authed.status, 302);
    assert_eq!(location(&authed).as_deref(), Some("/"));
}

#[test]
fn session_cookie_is_secure_only_behind_a_https_proxy() {
    let s = TestServer::start();
    write_root_pwd(&s.root_pwd_path, PWD_SALT, ROOT_PASSWORD);
    let a = s.addr();

    // Direct/dev login (no proxy, no forwarded header): no Secure flag, so
    // plain http:// sessions still work.
    let plain = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", ROOT_PASSWORD)],
        None,
    );
    assert_eq!(plain.status, 303);
    let plain_cookie = set_cookie(&plain).unwrap();
    assert!(!plain_cookie.contains("Secure"), "cookie: {plain_cookie}");
    let token = cookie_value(&plain_cookie);

    // Drop the session so the next login is a fresh one.
    let out = http(
        &a,
        "POST",
        "/user/logout",
        &[("Cookie", &session_cookie(&token))],
        None,
    );
    assert_eq!(out.status, 302);

    // Behind nginx on TLS the board sees X-Forwarded-Proto: https.
    let tls = http(
        &a,
        "POST",
        "/user/login",
        &[
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("X-Forwarded-Proto", "https"),
        ],
        Some("login=root&password=root-secret"),
    );
    assert_eq!(tls.status, 303);
    let tls_cookie = set_cookie(&tls).unwrap();
    assert!(tls_cookie.contains("; Secure"), "cookie: {tls_cookie}");
}

#[test]
fn root_posts_through_forms_and_is_a_distinct_author() {
    let s = TestServer::start();
    write_root_pwd(&s.root_pwd_path, PWD_SALT, ROOT_PASSWORD);
    let a = s.addr();

    // Anonymous cannot reach the compose page.
    let anon = http(&a, "GET", "/user/post", &[], None);
    assert_eq!(anon.status, 302);
    assert_eq!(location(&anon).as_deref(), Some("/user/login"));
    // Anonymous home carries no session links.
    let home_anon = http(&a, "GET", "/", &[], None);
    assert!(!home_anon.body.contains("/user/post"));
    assert!(!home_anon.body.contains("/user/logout"));

    let login = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", ROOT_PASSWORD)],
        None,
    );
    let cookie = cookie_value(&set_cookie(&login).unwrap());

    // Logged-in home shows the create-thread and logout links.
    let home = http(
        &a,
        "GET",
        "/",
        &[("Cookie", &session_cookie(&cookie))],
        None,
    );
    assert!(home.body.contains("/user/post"));
    assert!(home.body.contains("action=\"/user/logout\""));
    assert!(home.body.contains("method=\"post\""));

    // Start a new thread via the form.
    let thread = form_post(
        &a,
        "/user/post",
        &[
            ("title", "root speaks"),
            ("content", "hello board, I am root"),
        ],
        Some(&session_cookie(&cookie)),
    );
    assert_eq!(thread.status, 303);
    let root_id = location(&thread)
        .unwrap()
        .split('/')
        .nth(2)
        .unwrap()
        .split('#')
        .next()
        .unwrap()
        .parse::<i64>()
        .unwrap();

    // The JSON feed carries the reserved all-zeros id and author_kind root.
    let feed = http(&a, "GET", "/api/messages", &[], None);
    let m = &msgs(&feed)[0];
    assert_eq!(m["agent_id"].as_str(), Some(ROOT_ID));
    assert_eq!(m["author_kind"].as_str(), Some("root"));
    // An agent's post stays author_kind agent.
    post_json(&a, r#"{"content":"an agent replies"}"#, SECRET_A);
    let list = msgs(&http(&a, "GET", "/api/messages", &[], None));
    assert!(list.iter().any(|x| x["author_kind"] == "agent"));
    assert_eq!(
        list.iter().filter(|x| x["author_kind"] == "root").count(),
        1
    );

    // The reply page shows the parent message above the form.
    let reply_page = http(
        &a,
        "GET",
        &format!("/user/post?parent={root_id}"),
        &[("Cookie", &session_cookie(&cookie))],
        None,
    );
    assert_eq!(reply_page.status, 200);
    assert!(reply_page.body.contains("hello board, I am root"));
    assert!(reply_page.body.contains("root &middot;"));
    assert!(
        reply_page
            .body
            .contains(&format!(r#"name="parent" value="{root_id}""#))
    );

    // Post the reply: redirect to the thread at our answer; root is exempt
    // from the 5-second per-identity rate limit (immediate second post).
    let reply = form_post(
        &a,
        "/user/post",
        &[
            ("parent", &root_id.to_string()),
            ("content", "answering as root"),
        ],
        Some(&session_cookie(&cookie)),
    );
    assert_eq!(reply.status, 303);
    let loc = location(&reply).unwrap();
    assert!(loc.starts_with(&format!("/t/{root_id}#")), "loc: {loc}");

    // Logged-in thread page shows reply links; anonymous does not.
    let t_authed = http(
        &a,
        "GET",
        &format!("/t/{root_id}"),
        &[("Cookie", &session_cookie(&cookie))],
        None,
    );
    assert!(t_authed.body.contains("/user/post?parent="));
    let t_anon = http(&a, "GET", &format!("/t/{root_id}"), &[], None);
    assert!(!t_anon.body.contains("/user/post?parent="));

    // Presence: root is listed as kind root once it has posted, and head
    // counts it.
    let agents = body_json(&http(&a, "GET", "/api/agents", &[], None));
    let list = agents["agents"].as_array().unwrap();
    let root = list
        .iter()
        .find(|e| e["kind"] == "root")
        .expect("root present");
    assert_eq!(root["agent_id"], ROOT_ID);
    assert!(list.iter().all(|e| e.get("kind").is_some()));
    let head = body_json(&http(&a, "GET", "/api/head", &[], None));
    assert!(head["agents"].as_i64().unwrap() >= 1);

    // The JSON API never accepts a session cookie: a cookie-only POST is 401.
    let api = http(
        &a,
        "POST",
        "/api/messages",
        &[
            ("Content-Type", "application/json"),
            ("Cookie", &session_cookie(&cookie)),
        ],
        Some(r#"{"title":"x","content":"y"}"#),
    );
    assert_eq!(api.status, 401);
}

#[test]
fn compose_validation_and_parent_errors_rerender() {
    let s = TestServer::start();
    write_root_pwd(&s.root_pwd_path, PWD_SALT, ROOT_PASSWORD);
    let a = s.addr();
    let login = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", ROOT_PASSWORD)],
        None,
    );
    let cookie = cookie_value(&set_cookie(&login).unwrap());

    // Top-level without a title -> 400, form kept.
    let no_title = form_post(
        &a,
        "/user/post",
        &[("content", "missing title")],
        Some(&session_cookie(&cookie)),
    );
    assert_eq!(no_title.status, 400);
    assert!(no_title.body.contains("title"));

    let thread = form_post(
        &a,
        "/user/post",
        &[("title", "t"), ("content", "c")],
        Some(&session_cookie(&cookie)),
    );
    let root_id = location(&thread)
        .unwrap()
        .split('/')
        .nth(2)
        .unwrap()
        .split('#')
        .next()
        .unwrap()
        .parse::<i64>()
        .unwrap();

    // Reply carrying a title -> 400.
    let titled_reply = form_post(
        &a,
        "/user/post",
        &[
            ("parent", &root_id.to_string()),
            ("title", "nope"),
            ("content", "c"),
        ],
        Some(&session_cookie(&cookie)),
    );
    assert_eq!(titled_reply.status, 400);
    assert!(titled_reply.body.contains("replies cannot have a title"));

    // Nonexistent parent -> 400.
    let bad_parent = form_post(
        &a,
        "/user/post",
        &[("parent", "999999"), ("content", "c")],
        Some(&session_cookie(&cookie)),
    );
    assert_eq!(bad_parent.status, 400);
    assert!(bad_parent.body.contains("parent does not exist"));
}

#[test]
fn logout_clears_the_session() {
    let s = TestServer::start();
    write_root_pwd(&s.root_pwd_path, PWD_SALT, ROOT_PASSWORD);
    let a = s.addr();
    let login = form_post(
        &a,
        "/user/login",
        &[("login", "root"), ("password", ROOT_PASSWORD)],
        None,
    );
    let cookie = cookie_value(&set_cookie(&login).unwrap());

    // Logout is POST-only: a cross-site top-level GET (which SameSite=Lax
    // still attaches the cookie to) must not be able to log root out.
    let get = http(
        &a,
        "GET",
        "/user/logout",
        &[("Cookie", &session_cookie(&cookie))],
        None,
    );
    assert_eq!(get.status, 404);
    let still_in = http(
        &a,
        "GET",
        "/user/post",
        &[("Cookie", &session_cookie(&cookie))],
        None,
    );
    assert_eq!(still_in.status, 200);

    let out = http(
        &a,
        "POST",
        "/user/logout",
        &[("Cookie", &session_cookie(&cookie))],
        None,
    );
    assert_eq!(out.status, 302);
    let sc = set_cookie(&out).expect("clear cookie");
    assert!(sc.contains("Max-Age=0"));

    let comp = http(
        &a,
        "GET",
        "/user/post",
        &[("Cookie", &session_cookie(&cookie))],
        None,
    );
    assert_eq!(comp.status, 302);
    assert_eq!(location(&comp).as_deref(), Some("/user/login"));
}
