use glimpse_core::Config;
use glimpse_idle::{app, cli, runtime};
use tracing_subscriber::EnvFilter;

fn main() -> anyhow::Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    match argv.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }
        Some("--version") | Some("-V") => {
            println!("glimpse-idle {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("watch") => {
            let rest = argv[1..].to_vec();
            if rest.iter().any(|a| a == "--help" || a == "-h") {
                print_watch_help();
                return Ok(());
            }
            let json = rest.iter().any(|a| a == "--json");
            let patterns: Vec<String> = rest.into_iter().filter(|a| a != "--json").collect();
            let patterns = if patterns.is_empty() { vec!["*".to_owned()] } else { patterns };
            return run_async(cli::watch(cli::WatchArgs { patterns, json }));
        }
        Some("dispatch") => {
            let rest_raw = &argv[1..];
            if rest_raw.iter().any(|a| a == "--help" || a == "-h") {
                print_dispatch_help();
                return Ok(());
            }
            let json = rest_raw.iter().any(|a| a == "--json");
            let rest: Vec<String> = rest_raw.iter().filter(|a| a.as_str() != "--json").cloned().collect();
            let command = rest
                .first()
                .ok_or_else(|| anyhow::anyhow!("dispatch: command name required"))?
                .clone();
            let fields = rest[1..].to_vec();
            return run_async(cli::dispatch(cli::DispatchArgs { command, fields, json }));
        }
        None => {}
        Some(unknown) => {
            eprintln!("glimpse-idle: unknown command '{unknown}'");
            eprintln!("Try 'glimpse-idle --help' for usage.");
            std::process::exit(1);
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(log_filter())
        .init();
    tracing::info!("glimpse-idle {}", env!("CARGO_PKG_VERSION"));

    let config = Config::load();
    tracing::debug!(
        enabled = config.idle.enabled,
        respect_inhibitors = config.idle.respect_inhibitors,
        ac_listeners = config.idle.profiles.ac.listeners.len(),
        battery_listeners = config.idle.profiles.battery.listeners.len(),
        "resolved startup idle configuration"
    );

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let instance = match runtime::acquire_single_instance().await {
                Ok(guard) => {
                    tracing::info!("acquired single-instance D-Bus name");
                    guard
                }
                Err(error) => {
                    tracing::error!("failed to start glimpse-idle: {error}");
                    return Err(error);
                }
            };

            app::run(config, instance).await
        })
}

fn run_async<F>(f: F) -> anyhow::Result<()>
where
    F: std::future::Future<Output = anyhow::Result<()>>,
{
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(f)
}

fn print_help() {
    println!("glimpse-idle {}", env!("CARGO_PKG_VERSION"));
    println!("Glimpse idle inhibitor daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-idle [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    watch      Subscribe to idle inhibitor events");
    println!("    dispatch   Send a command to the running daemon");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help      Print help");
    println!("    -V, --version   Print version");
    println!();
    println!("Without a command, glimpse-idle starts the daemon.");
}

fn print_watch_help() {
    println!("glimpse-idle-watch");
    println!("Subscribe to idle inhibitor events");
    println!();
    println!("USAGE:");
    println!("    glimpse-idle watch [OPTIONS] [<pattern>...]");
    println!();
    println!("ARGS:");
    println!("    <pattern>...   Event patterns to subscribe to (default: *)");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print each event as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("EVENTS:");
    println!("    idle.inhibitor_added    New idle inhibitor registered");
    println!("    idle.inhibitor_removed  Idle inhibitor released");
    println!("    idle.backend_health_changed  Backend health changed (screen_saver|portal|login1)");
}

fn print_dispatch_help() {
    println!("glimpse-idle-dispatch");
    println!("Send a command to the running daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-idle dispatch [OPTIONS] <COMMAND> [key=value...]");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print the ack as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("COMMANDS:");
    println!("    release id=<id>   Force-release an idle inhibitor by numeric ID");
}

fn log_filter() -> EnvFilter {
    match std::env::var("GLIMPSE_LOG_LEVEL") {
        Ok(value) => {
            normalized_glimpse_log_filter(&value).unwrap_or_else(|| EnvFilter::new("info"))
        }
        Err(_) => EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
    }
}

fn normalized_glimpse_log_filter(value: &str) -> Option<EnvFilter> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    EnvFilter::try_new(value).ok()
}

#[cfg(test)]
mod tests {
    use super::normalized_glimpse_log_filter;

    #[test]
    fn bare_glimpse_log_level_is_accepted() {
        let filter = normalized_glimpse_log_filter("debug").unwrap();

        assert!(filter.to_string().contains("debug"));
    }
}
