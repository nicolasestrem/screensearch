//! `screensearch-mcp` binary entry point (0.3.0 PR8; `03 §7c`, D13).
//!
//! Resolves config from args/env, builds a single-threaded tokio runtime, and runs the
//! stdio MCP loop. Everything that isn't a protocol message goes to stderr.

use std::io;

use mcp::client::ApiClient;
use mcp::config::{self, Cli};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match config::parse(&args, |k| std::env::var(k).ok()) {
        Cli::Help => println!("{}", config::USAGE),
        Cli::Version => println!("screensearch-mcp {}", env!("CARGO_PKG_VERSION")),
        Cli::BadUsage(msg) => {
            eprintln!("screensearch-mcp: {msg}\n\n{}", config::USAGE);
            std::process::exit(2);
        }
        Cli::Run(cfg) => run(cfg),
    }
}

fn run(cfg: config::Config) {
    if cfg.token.is_none() {
        eprintln!(
            "[screensearch-mcp] warning: no API token configured (SCREENSEARCH_API_TOKEN unset \
             and --token not given); tool calls will return a guidance error until it is set."
        );
    }

    // A current-thread runtime is enough: requests are processed sequentially and the only
    // long call (`ask`) is bounded by its own timeout. `net` is enabled so `enable_all`
    // installs the IO driver reqwest needs.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    let client = match ApiClient::new(cfg.url, cfg.token) {
        Ok(client) => client.with_read_only(cfg.read_only),
        Err(message) => {
            eprintln!("screensearch-mcp: {message}");
            std::process::exit(2);
        }
    };
    let stdin = io::stdin();
    let stdout = io::stdout();
    mcp::run(stdin.lock(), stdout.lock(), &rt, &client);
}
