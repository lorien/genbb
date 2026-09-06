use std::time::Duration;

use genbb::{BoardServer, DEFAULT_DB, DEFAULT_HOST, DEFAULT_PORT, DEFAULT_WORKERS};

fn main() {
    let mut host = DEFAULT_HOST.to_string();
    let mut port = DEFAULT_PORT;
    let mut db = DEFAULT_DB.to_string();
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
            "--workers" => match args.next().and_then(|v| v.parse().ok()) {
                Some(v) => workers = v,
                None => fatal("--workers needs a number"),
            },
            other => fatal(&format!("unknown argument: {other}")),
        }
    }
    let server = match BoardServer::start(&host, port, &db, workers) {
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
