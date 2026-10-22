use glimpse_config::Documents;

fn main() {
    let documents = match std::env::args().nth(1).as_deref() {
        Some("config") => Documents::Config,
        Some("panel") => Documents::ConfigAndPanel,
        _ => std::process::exit(2),
    };
    print!("{}", glimpse_config::commented_document(documents));
}
