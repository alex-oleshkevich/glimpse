use std::path::{Path, PathBuf};

use clap::Parser;

use super::ConfigArg;

#[derive(Parser)]
#[command(about=None, long_about=None)]
struct Cli {
    #[command(flatten)]
    config: ConfigArg,
}

fn parse(argv: &[&str]) -> Result<ConfigArg, clap::Error> {
    Cli::try_parse_from(argv).map(|cli| cli.config)
}

#[test]
fn config_dir_flag_sets_the_directory() {
    let dir = PathBuf::from("/tmp/glimpse-test");
    let args = parse(&["glimpse", "--config-dir", dir.to_str().unwrap()]).unwrap();
    assert_eq!(
        args.config_dir.as_deref(),
        Some(Path::new("/tmp/glimpse-test"))
    );
}

#[test]
fn short_flag_is_rejected() {
    assert!(parse(&["glimpse", "-c", "/x"]).is_err());
}

#[test]
fn config_file_flag_is_rejected() {
    assert!(parse(&["glimpse", "--config", "/x"]).is_err());
}

#[test]
fn an_empty_config_dir_is_accepted_for_init_to_treat_as_unset() {
    assert!(parse(&["glimpse", "--config-dir", ""]).is_ok());
}
