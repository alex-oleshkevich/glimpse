fn main() {
    println!("cargo:rerun-if-env-changed=PREFIX");
    let prefix = std::env::var("PREFIX").unwrap_or_else(|_| "/usr".to_owned());
    println!("cargo:rustc-env=GLIMPSE_LOCALE_DIR_DEFAULT={prefix}/share/locale");
}
