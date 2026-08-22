use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
#[value(rename_all = "lower")]
pub enum LogFormat {
    #[default]
    Plain,
    Json,
}

pub fn init_app_tracing(level: &str, format: LogFormat) {
    let env_filter = match tracing_subscriber::EnvFilter::try_new(level) {
        Ok(env_filter) => env_filter,
        Err(error) => {
            eprintln!("ignoring invalid log filter {level:?}: {error}");
            tracing_subscriber::EnvFilter::new("info")
        }
    };

    let color = anstream::AutoStream::choice(&std::io::stderr()) != anstream::ColorChoice::Never;
    let builder = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_ansi(color);

    match format {
        LogFormat::Plain => builder.init(),
        LogFormat::Json => builder.json().init(),
    }
}
