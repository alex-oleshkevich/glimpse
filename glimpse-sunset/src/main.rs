use anyhow::Result;
use glimpse_core::Config;
use glimpse_sunset::{app, cli, runtime};
use tracing_subscriber::EnvFilter;

fn main() -> Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    match argv.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }
        Some("--version") | Some("-V") => {
            println!("glimpse-sunset {}", env!("CARGO_PKG_VERSION"));
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
            let patterns = if patterns.is_empty() {
                vec!["*".to_owned()]
            } else {
                patterns
            };
            return run_async(cli::watch(cli::WatchArgs { patterns, json }));
        }
        Some("dispatch") => {
            let rest_raw = &argv[1..];
            if rest_raw.iter().any(|a| a == "--help" || a == "-h") {
                print_dispatch_help();
                return Ok(());
            }
            let json = rest_raw.iter().any(|a| a == "--json");
            let rest: Vec<String> = rest_raw
                .iter()
                .filter(|a| a.as_str() != "--json")
                .cloned()
                .collect();
            let command = rest
                .first()
                .ok_or_else(|| anyhow::anyhow!("dispatch: command name required"))?
                .clone();
            if let Some((cmd, _)) = command.split_once('=') {
                eprintln!("glimpse-sunset: dispatch command must not contain '='");
                eprintln!("  got:  dispatch {command}");
                eprintln!("  try:  dispatch {cmd} <key>=<value>");
                eprintln!("Run 'glimpse-sunset dispatch --help' for available commands.");
                std::process::exit(1);
            }
            let fields = rest[1..].to_vec();
            return run_async(cli::dispatch(cli::DispatchArgs {
                command,
                fields,
                json,
            }));
        }
        None => {}
        Some(unknown) => {
            eprintln!("glimpse-sunset: unknown command '{unknown}'");
            eprintln!("Try 'glimpse-sunset --help' for usage.");
            std::process::exit(1);
        }
    }

    run_daemon()
}

fn run_async<F: std::future::Future<Output = Result<()>>>(f: F) -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(f)
}

fn run_daemon() -> Result<()> {
    let filter = log_filter();
    tracing_subscriber::fmt().with_env_filter(filter).init();
    tracing::info!("glimpse-sunset {}", env!("CARGO_PKG_VERSION"));

    let config = Config::load();
    tracing::debug!(
        schedule = ?config.night_light.schedule,
        temperature_kelvin = config.night_light.temperature,
        transition_minutes = config.night_light.transition_minutes,
        "resolved startup configuration"
    );
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            let _single_instance = match runtime::acquire_single_instance().await {
                Ok(guard) => {
                    tracing::info!("acquired single-instance D-Bus name");
                    guard
                }
                Err(error) => {
                    tracing::error!("failed to start glimpse-sunset: {error}");
                    return Err(error);
                }
            };

            app::run(config).await
        })
}

fn print_help() {
    println!("glimpse-sunset {}", env!("CARGO_PKG_VERSION"));
    println!("Glimpse night-light daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-sunset [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    watch      Subscribe to night-light events from the running daemon");
    println!("    dispatch   Send a command to the running daemon");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help      Print help");
    println!("    -V, --version   Print version");
    println!();
    println!("Without a command, glimpse-sunset starts the daemon.");
    println!("Run 'glimpse-sunset <COMMAND> --help' for subcommand help.");
}

fn print_watch_help() {
    println!("glimpse-sunset-watch");
    println!("Subscribe to night-light events from the running daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-sunset watch [OPTIONS] [<pattern>...]");
    println!();
    println!("ARGS:");
    println!("    <pattern>...   Event patterns to subscribe to (default: *)");
    println!("                   Forms: '*', 'nightlight.*', 'nightlight.phase_changed'");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print each event as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("EVENTS:");
    println!(
        "    nightlight.phase_changed      phase=<disabled|day|transition_to_night|night|transition_to_day>"
    );
    println!("    nightlight.activated          temperature=<kelvin>");
    println!("    nightlight.deactivated");
    println!("    nightlight.temperature_changed  kelvin=<u32> phase=<phase>");
    println!(
        "    nightlight.health_changed     health=<ready|starting|unsupported|reconnecting|degraded>"
    );
}

fn print_dispatch_help() {
    println!("glimpse-sunset-dispatch");
    println!("Send a command to the running daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-sunset dispatch [OPTIONS] <COMMAND> [key=value...]");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print the ack as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("COMMANDS:");
    println!("    status                                     Show current state");
    println!("    solar                                      Show solar/sunrise/sunset times");
    println!("    refresh                                    Refresh location and solar data");
    println!(
        "    activate                                   Activate night light immediately (overrides schedule)"
    );
    println!("    enable                                     Enable (set schedule=automatic)");
    println!("    disable                                    Disable (set schedule=off)");
    println!("    set_temperature kelvin=<u32>               Set colour temperature");
    println!("    set_schedule schedule=<off|automatic|schedule>  Set schedule mode");
    println!("    set_times start=<HH:MM> end=<HH:MM>       Set manual schedule window");
    println!("    set_location lat=<f64> lon=<f64>          Override geolocation");
    println!("    reset                                      Reset temperature to config default");
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
    use super::{normalized_glimpse_log_filter, print_dispatch_help, print_help, print_watch_help};

    #[test]
    fn help_does_not_panic() {
        print_help();
        print_watch_help();
        print_dispatch_help();
    }

    #[test]
    fn bare_glimpse_log_level_is_accepted() {
        let filter = normalized_glimpse_log_filter("debug").unwrap();
        assert!(filter.to_string().contains("debug"));
    }

    #[test]
    fn empty_log_level_returns_none() {
        assert!(normalized_glimpse_log_filter("").is_none());
        assert!(normalized_glimpse_log_filter("   ").is_none());
    }
}
