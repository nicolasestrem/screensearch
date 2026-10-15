//! Command-line + environment resolution for `screensearch-mcp`.
//!
//! The binary's entire configuration is the local API base URL and the bearer token
//! (`03 §7c`, D13). Precedence is **flag > environment variable > built-in default**.
//! Both `--flag value` and `--flag=value` forms are accepted.

/// Environment variable naming the API base URL (`03 §7c`).
pub const ENV_URL: &str = "SCREENSEARCH_API_URL";
/// Environment variable carrying the API bearer token (`03 §7c`).
pub const ENV_TOKEN: &str = "SCREENSEARCH_API_TOKEN";
/// The default base URL: loopback on the API's default port (`api::DEFAULT_PORT` = 43210).
/// The literal is kept local so the binary never links `crates/api` (D13).
pub const DEFAULT_URL: &str = "http://127.0.0.1:43210";

/// Resolved runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub url: String,
    pub token: Option<String>,
}

/// The outcome of parsing the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    /// Run the server with this configuration.
    Run(Config),
    /// Print help to stdout and exit 0.
    Help,
    /// Print the version to stdout and exit 0.
    Version,
    /// An unknown flag or a flag missing its value; print usage to stderr and exit 2.
    BadUsage(String),
}

/// Human-readable usage, shown by `--help` and on a bad invocation.
pub const USAGE: &str = "\
screensearch-mcp — a stdio MCP server wrapping the ScreenSearch local API.

USAGE:
    screensearch-mcp [--url <URL>] [--token <TOKEN>]

OPTIONS:
    --url <URL>       Base URL of the local API (default: http://127.0.0.1:43210)
    --token <TOKEN>   Bearer token from ScreenSearch Settings -> Local API
    --version         Print version and exit
    --help            Print this help and exit

ENVIRONMENT:
    SCREENSEARCH_API_URL     Same as --url (flag wins)
    SCREENSEARCH_API_TOKEN   Same as --token (flag wins)

The API is off by default; enable it in ScreenSearch Settings -> Local API and copy the
token shown there. Any local process holding the token can read your entire screen
history — enabling the API is an explicit trust decision.";

/// Parses `args` (process args **without** argv[0]) against an environment lookup.
pub fn parse(args: &[String], env: impl Fn(&str) -> Option<String>) -> Cli {
    let mut url_flag: Option<String> = None;
    let mut token_flag: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        // Split `--key=value` at most once.
        let (key, inline) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        match key {
            "--help" | "-h" => return Cli::Help,
            "--version" | "-V" => return Cli::Version,
            "--url" | "--token" => {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        match args.get(i) {
                            Some(v) => v.clone(),
                            None => return Cli::BadUsage(format!("{key} requires a value")),
                        }
                    }
                };
                if key == "--url" {
                    url_flag = Some(value);
                } else {
                    token_flag = Some(value);
                }
            }
            other => return Cli::BadUsage(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    let url = url_flag
        .or_else(|| env(ENV_URL))
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    // Trim a trailing slash so `{url}/v1/...` never doubles up.
    let url = url.trim_end_matches('/').to_string();
    // MCP intentionally remains a local client even though the application API serves
    // all IPv4 interfaces. Reject remote destinations to avoid leaking the token through
    // a typo or copied config; remote MCP needs a separate contract amendment (`03 §7c`).
    if !is_loopback_url(&url) {
        return Cli::BadUsage(format!(
            "--url must be a loopback address (127.0.0.1 / localhost / [::1]); got `{url}`"
        ));
    }
    let token = token_flag
        .or_else(|| env(ENV_TOKEN))
        .filter(|t| !t.is_empty());

    Cli::Run(Config { url, token })
}

/// Whether `raw` is an `http`/`https` URL whose host is the loopback interface.
///
/// Uses `reqwest`'s URL parser (already linked) so IPv6 brackets, ports, and userinfo are
/// handled correctly; `host_str()` avoids naming the `url::Host` type so the crate needs no
/// direct `url` dependency. An unparseable URL is not loopback.
fn is_loopback_url(raw: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(raw) else {
        return false;
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        return false;
    }
    match parsed.host_str() {
        Some(host) if host.eq_ignore_ascii_case("localhost") => true,
        Some(host) => {
            // `host_str()` keeps an IPv6 literal in brackets — strip them before parsing.
            let bare = host
                .strip_prefix('[')
                .and_then(|s| s.strip_suffix(']'))
                .unwrap_or(host);
            bare.parse::<std::net::IpAddr>()
                .map(|ip| ip.is_loopback())
                .unwrap_or(false)
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn run(cli: Cli) -> Config {
        match cli {
            Cli::Run(c) => c,
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn defaults_when_nothing_set() {
        let c = run(parse(&[], no_env));
        assert_eq!(c.url, DEFAULT_URL);
        assert_eq!(c.token, None);
    }

    #[test]
    fn flag_overrides_env_overrides_default() {
        let env = |k: &str| match k {
            ENV_URL => Some("http://127.0.0.1:9".to_string()),
            ENV_TOKEN => Some("env-token".to_string()),
            _ => None,
        };
        // Env only.
        let c = run(parse(&[], env));
        assert_eq!(c.url, "http://127.0.0.1:9");
        assert_eq!(c.token.as_deref(), Some("env-token"));

        // Flags win over env.
        let args = vec![
            "--url".to_string(),
            "http://127.0.0.1:1".to_string(),
            "--token".to_string(),
            "flag-token".to_string(),
        ];
        let c = run(parse(&args, env));
        assert_eq!(c.url, "http://127.0.0.1:1");
        assert_eq!(c.token.as_deref(), Some("flag-token"));
    }

    #[test]
    fn equals_and_space_flag_forms() {
        let space = run(parse(
            &["--url".to_string(), "http://127.0.0.1:2".to_string()],
            no_env,
        ));
        let equals = run(parse(&["--url=http://127.0.0.1:2".to_string()], no_env));
        assert_eq!(space.url, equals.url);
        assert_eq!(space.url, "http://127.0.0.1:2");
    }

    #[test]
    fn loopback_hosts_are_accepted() {
        for u in [
            "http://127.0.0.1:43210",
            "http://localhost:43210",
            "http://[::1]:43210",
            "http://127.9.9.9:1",
        ] {
            assert!(
                matches!(parse(&[format!("--url={u}")], no_env), Cli::Run(_)),
                "{u} should be accepted"
            );
        }
    }

    #[test]
    fn non_loopback_url_is_rejected() {
        for u in [
            "http://example.com:43210",
            "http://10.0.0.5:43210",
            "http://0.0.0.0:43210",
            "not-a-url",
        ] {
            assert!(
                matches!(parse(&[format!("--url={u}")], no_env), Cli::BadUsage(_)),
                "{u} should be rejected"
            );
        }
    }

    #[test]
    fn trailing_slash_trimmed() {
        let c = run(parse(
            &["--url=http://127.0.0.1:43210/".to_string()],
            no_env,
        ));
        assert_eq!(c.url, "http://127.0.0.1:43210");
    }

    #[test]
    fn empty_token_is_none() {
        let env = |k: &str| (k == ENV_TOKEN).then(String::new);
        assert_eq!(run(parse(&[], env)).token, None);
    }

    #[test]
    fn help_and_version_short_circuit() {
        assert_eq!(parse(&["--help".to_string()], no_env), Cli::Help);
        assert_eq!(parse(&["--version".to_string()], no_env), Cli::Version);
    }

    #[test]
    fn unknown_flag_and_missing_value_are_bad_usage() {
        assert!(matches!(
            parse(&["--nope".to_string()], no_env),
            Cli::BadUsage(_)
        ));
        assert!(matches!(
            parse(&["--url".to_string()], no_env),
            Cli::BadUsage(_)
        ));
    }
}
