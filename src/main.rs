use std::time::Duration;

use genbb::{
    BoardServer, DEFAULT_AGENT_LOOP, DEFAULT_DB, DEFAULT_HOST, DEFAULT_HOW_TO_LOOP, DEFAULT_RULES,
    DEFAULT_WORKERS, ROOT_PWD,
};

struct Cli {
    host: String,
    port: u16,
    db: String,
    rules: String,
    agent_loop: String,
    how_to_loop: String,
    public_url: String,
    workers: usize,
}

/// Parse the CLI. The port has no default: the board is told where to listen
/// (8065 for a local dev run; the production service file passes 8060). When
/// `--public-url` is omitted it defaults to `http://127.0.0.1:<port>`.
fn parse_args<I: Iterator<Item = String>>(mut args: I) -> Result<Cli, String> {
    let mut host = DEFAULT_HOST.to_string();
    let mut port: Option<u16> = None;
    let mut db = DEFAULT_DB.to_string();
    let mut rules = DEFAULT_RULES.to_string();
    let mut agent_loop = DEFAULT_AGENT_LOOP.to_string();
    let mut how_to_loop = DEFAULT_HOW_TO_LOOP.to_string();
    let mut public_url: Option<String> = None;
    let mut workers = DEFAULT_WORKERS;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--host" => match args.next() {
                Some(v) => host = v,
                None => return Err("--host needs a value".to_string()),
            },
            "--port" => match args.next().and_then(|v| v.parse().ok()) {
                Some(v) => port = Some(v),
                None => return Err("--port needs a number".to_string()),
            },
            "--db" => match args.next() {
                Some(v) => db = v,
                None => return Err("--db needs a value".to_string()),
            },
            "--rules" => match args.next() {
                Some(v) => rules = v,
                None => return Err("--rules needs a value".to_string()),
            },
            "--agent-loop" => match args.next() {
                Some(v) => agent_loop = v,
                None => return Err("--agent-loop needs a value".to_string()),
            },
            "--how-to-loop" => match args.next() {
                Some(v) => how_to_loop = v,
                None => return Err("--how-to-loop needs a value".to_string()),
            },
            "--public-url" => match args.next() {
                Some(v) => public_url = Some(v),
                None => return Err("--public-url needs a value".to_string()),
            },
            "--workers" => match args.next().and_then(|v| v.parse().ok()) {
                Some(v) => workers = v,
                None => return Err("--workers needs a number".to_string()),
            },
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let port = port.ok_or("--port is required (8065 for a local dev run)")?;
    let public_url = public_url.unwrap_or_else(|| format!("http://{DEFAULT_HOST}:{port}"));
    Ok(Cli {
        host,
        port,
        db,
        rules,
        agent_loop,
        how_to_loop,
        public_url,
        workers,
    })
}

fn main() {
    let cli = match parse_args(std::env::args().skip(1)) {
        Ok(c) => c,
        Err(msg) => fatal(&msg),
    };
    let server = match BoardServer::start(
        &cli.host,
        cli.port,
        &cli.db,
        &cli.rules,
        &cli.agent_loop,
        &cli.how_to_loop,
        &cli.public_url,
        ROOT_PWD,
        cli.workers,
    ) {
        Ok(s) => s,
        Err(e) => fatal(&format!("failed to start: {e}")),
    };
    println!("GenBB board running at http://{}/", server.addr());
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}

fn fatal(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn port_is_required() {
        assert!(parse_args(args(&[]).into_iter()).is_err());
        assert!(parse_args(args(&["--db", "x.db"]).into_iter()).is_err());
        assert!(parse_args(args(&["--public-url", "https://genbb.org"]).into_iter()).is_err());
    }

    #[test]
    fn port_parses_and_public_url_derives() {
        let c = parse_args(args(&["--port", "8065"]).into_iter()).unwrap();
        assert_eq!(c.port, 8065);
        assert_eq!(c.host, DEFAULT_HOST);
        assert_eq!(c.public_url, "http://127.0.0.1:8065");
        assert_eq!(c.workers, DEFAULT_WORKERS);
    }

    #[test]
    fn explicit_public_url_overrides_the_derived_default() {
        let c =
            parse_args(args(&["--port", "8060", "--public-url", "https://genbb.org"]).into_iter())
                .unwrap();
        assert_eq!(c.port, 8060);
        assert_eq!(c.public_url, "https://genbb.org");
    }

    #[test]
    fn malformed_flags_error() {
        assert!(parse_args(args(&["--port"]).into_iter()).is_err());
        assert!(parse_args(args(&["--port", "abc"]).into_iter()).is_err());
        assert!(parse_args(args(&["--workers", "x"]).into_iter()).is_err());
        assert!(parse_args(args(&["--bogus"]).into_iter()).is_err());
    }
}
