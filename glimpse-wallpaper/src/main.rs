use glimpse_core::Config;
use glimpse_wallpaper::{
    app::{AppInit, WallpaperAppModel},
    cli, ipc as wallpaper_ipc,
    runtime::{GTK_APPLICATION_ID, WallpaperRuntime},
};
use relm4::{
    RELM_THREADS, RelmApp,
    gtk::{self, gio::prelude::ApplicationExtManual},
};
use tracing_subscriber::EnvFilter;

const GTK_APPLICATION_ID_ENV: &str = "GLIMPSE_WALLPAPER_APP_ID";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let argv: Vec<String> = std::env::args().skip(1).collect();

    match argv.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print_help();
            return Ok(());
        }
        Some("--version") | Some("-V") => {
            println!("glimpse-wallpaper {}", env!("CARGO_PKG_VERSION"));
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
            return cli::watch(cli::WatchArgs { patterns, json }).await;
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
                eprintln!("glimpse-wallpaper: dispatch command must not contain '='");
                eprintln!("  got:  dispatch {command}");
                eprintln!("  try:  dispatch {cmd} <key>=<value>");
                eprintln!("Run 'glimpse-wallpaper dispatch --help' for available commands.");
                std::process::exit(1);
            }
            let fields = rest[1..].to_vec();
            return cli::dispatch(cli::DispatchArgs {
                command,
                fields,
                json,
            })
            .await;
        }
        None => {}
        Some(unknown) => {
            eprintln!("glimpse-wallpaper: unknown command '{unknown}'");
            eprintln!("Try 'glimpse-wallpaper --help' for usage.");
            std::process::exit(1);
        }
    }

    let filter = log_filter();
    tracing_subscriber::fmt().with_env_filter(filter).init();
    tracing::info!("glimpse-wallpaper {}", env!("CARGO_PKG_VERSION"));

    let threads = std::env::var("GLIMPSE_THREADS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    if RELM_THREADS.set(threads).is_err() {
        tracing::warn!(
            threads,
            "RELM_THREADS already initialized; GLIMPSE_THREADS ignored"
        );
    } else {
        tracing::debug!(threads, "configured Relm4 worker threads");
    }

    let app_id = gtk_application_id();
    let _single_instance = match WallpaperRuntime::acquire_single_instance_with_name(&app_id).await
    {
        Ok(guard) => {
            tracing::info!(app_id, "acquired single-instance D-Bus name");
            guard
        }
        Err(err) => {
            tracing::error!("failed to start glimpse-wallpaper: {err}");
            return Err(err);
        }
    };

    let config = Config::load();
    tracing::debug!(
        wallpaper_color = %config.wallpaper.color,
        wallpaper_path = config.wallpaper.path.as_ref().map(|path| path.display().to_string()).as_deref().unwrap_or("<none>"),
        backdrop_enabled = config.backdrop.enabled,
        "resolved startup configuration"
    );
    let (_ipc_handle, event_tx, command_rx) = wallpaper_ipc::start();
    let gtk_app = gtk::Application::builder()
        .application_id(&app_id)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let _app_hold = gtk_app.hold();
    tracing::debug!(app_id, "starting GTK application");
    let app = RelmApp::from_app(gtk_app);
    app.visible_on_activate(false)
        .run::<WallpaperAppModel>(AppInit {
            config,
            event_tx,
            command_rx,
        });
    tracing::info!("glimpse-wallpaper stopped");

    Ok(())
}

fn gtk_application_id() -> String {
    gtk_application_id_from_env(std::env::var(GTK_APPLICATION_ID_ENV).ok())
}

fn gtk_application_id_from_env(value: Option<String>) -> String {
    value.unwrap_or_else(|| GTK_APPLICATION_ID.into())
}

fn log_filter() -> EnvFilter {
    match std::env::var("GLIMPSE_LOG_LEVEL") {
        Ok(value) => normalized_glimpse_log_filter(&value)
            .unwrap_or_else(|| EnvFilter::new("info,relm4=warn")),
        Err(_) => {
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,relm4=warn"))
        }
    }
}

fn normalized_glimpse_log_filter(value: &str) -> Option<EnvFilter> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let filter = if value.contains(',') || value.contains('=') {
        value.to_string()
    } else {
        format!("{value},relm4=warn")
    };

    EnvFilter::try_new(filter).ok()
}

fn print_help() {
    println!("glimpse-wallpaper {}", env!("CARGO_PKG_VERSION"));
    println!("Glimpse wallpaper daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-wallpaper [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    watch      Subscribe to wallpaper events from the running daemon");
    println!("    dispatch   Send a command to the running daemon");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help      Print help");
    println!("    -V, --version   Print version");
    println!();
    println!("Without a command, glimpse-wallpaper starts the daemon.");
    println!("Run 'glimpse-wallpaper <COMMAND> --help' for subcommand help.");
}

fn print_watch_help() {
    println!("glimpse-wallpaper-watch");
    println!("Subscribe to wallpaper events from the running daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-wallpaper watch [OPTIONS] [<pattern>...]");
    println!();
    println!("ARGS:");
    println!("    <pattern>...   Event patterns to subscribe to (default: *)");
    println!("                   Forms: '*', 'wallpaper.*', 'wallpaper.spec_changed'");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print each event as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("EVENTS:");
    println!(
        "    wallpaper.spec_changed   mode=<image|color> color=<css> [path=<file> fit=<cover|contain|fill>] backdrop=<true|false> [backdrop_blur=<u32> backdrop_path=<file>]"
    );
    println!("    wallpaper.theme_changed  mode=<light|dark>");
}

fn print_dispatch_help() {
    println!("glimpse-wallpaper-dispatch");
    println!("Send a command to the running daemon");
    println!();
    println!("USAGE:");
    println!("    glimpse-wallpaper dispatch [OPTIONS] <COMMAND> [key=value...]");
    println!();
    println!("OPTIONS:");
    println!("    --json      Print the ack as a JSON object");
    println!("    -h, --help  Print help");
    println!();
    println!("COMMANDS:");
    println!("    reload_config                              Reload config from disk (clears runtime overrides)");
    println!("    set_image path=<abs file>                  Override the wallpaper image");
    println!("    set_color color=<css>                      Override the background colour");
    println!("    set_fit mode=<cover|contain|fill>          Override the image fit mode");
    println!(
        "    set_backdrop enabled=<bool> [path=<abs> blur=<u32>]  Override the backdrop"
    );
    println!("    set_theme_mode mode=<light|dark|auto>      Pin the theme (auto follows config/gsettings)");
    println!();
    println!("Runtime overrides are in-memory only and survive unrelated config edits;");
    println!("'reload_config' clears them and re-reads disk.");
}

#[cfg(test)]
mod tests {
    use super::{
        gtk_application_id_from_env, normalized_glimpse_log_filter, print_dispatch_help,
        print_help, print_watch_help,
    };

    #[test]
    fn bare_glimpse_log_level_keeps_relm4_quiet() {
        let filter = normalized_glimpse_log_filter("debug").unwrap();
        let filter = filter.to_string();

        assert!(filter.contains("debug"));
        assert!(filter.contains("relm4=warn"));
    }

    #[test]
    fn explicit_glimpse_log_filter_is_preserved() {
        let filter = normalized_glimpse_log_filter("info,relm4=debug").unwrap();
        let filter = filter.to_string();

        assert!(filter.contains("info"));
        assert!(filter.contains("relm4=debug"));
    }

    #[test]
    fn wallpaper_app_id_defaults_to_runtime_constant() {
        assert_eq!(gtk_application_id_from_env(None), super::GTK_APPLICATION_ID);
    }

    #[test]
    fn wallpaper_app_id_can_be_overridden_from_env() {
        assert_eq!(
            gtk_application_id_from_env(Some("me.aresa.GlimpseWallpaper.TestApp".into())),
            "me.aresa.GlimpseWallpaper.TestApp"
        );
    }

    #[test]
    fn help_text_does_not_panic() {
        print_help();
        print_watch_help();
        print_dispatch_help();
    }
}
