use std::time::Duration;

use genbb::{
    BoardServer, DEFAULT_AGENT_LOOP, DEFAULT_DB, DEFAULT_HOST, DEFAULT_HOW_TO_LOOP, DEFAULT_PORT,
    DEFAULT_PUBLIC_URL, DEFAULT_RULES, DEFAULT_WORKERS,
};

fn main() {
    let mut host = DEFAULT_HOST.to_string();
    let mut port = DEFAULT_PORT;
    let mut db = DEFAULT_DB.to_string();
    let mut rules = DEFAULT_RULES.to_string();
    let mut agent_loop = DEFAULT_AGENT_LOOP.to_string();
    let mut how_to_loop = DEFAULT_HOW_TO_LOOP.to_string();
    let mut public_url = DEFAULT_PUBLIC_URL.to_string();
    let mut workers = DEFAULT_WORKERS;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => match args.next() {
                Some(v) => host = v,
                None => fatal("--host needs a value"),
            },
            "--port" => match args.next().and_then(|v| v.parse().ok()) {
                Some(v) => port = v,
                None => fatal("--port needs a number"),
            },
            "--db" => match args.next() {
                Some(v) => db = v,
                None => fatal("--db needs a value"),
            },
            "--rules" => match args.next() {
                Some(v) => rules = v,
                None => fatal("--rules needs a value"),
            },
            "--agent-loop" => match args.next() {
                Some(v) => agent_loop = v,
                None => fatal("--agent-loop needs a value"),
            },
            "--how-to-loop" => match args.next() {
                Some(v) => how_to_loop = v,
                None => fatal("--how-to-loop needs a value"),
            },
            "--public-url" => match args.next() {
                Some(v) => public_url = v,
                None => fatal("--public-url needs a value"),
            },
            "--workers" => match args.next().and_then(|v| v.parse().ok()) {
                Some(v) => workers = v,
                None => fatal("--workers needs a number"),
            },
            other => fatal(&format!("unknown argument: {other}")),
        }
    }
    let server = match BoardServer::start(
        &host,
        port,
        &db,
        &rules,
        &agent_loop,
        &how_to_loop,
        &public_url,
        workers,
    ) {
        Ok(s) => s,
        Err(e) => fatal(&format!("failed to start: {e}")),
    };
    println!("GenBB board running at http://{host}:{}/", server.port());
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn fatal(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}
