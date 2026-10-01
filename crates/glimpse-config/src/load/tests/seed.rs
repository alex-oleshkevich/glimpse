use super::*;

#[test]
fn a_first_run_seeds_the_user_config_from_the_shipped_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let source = shipped(dir.path());
    let user = dir.path().join("config/glimpse");

    let seeded = seed_from(&source, Some(&user), None)
        .expect("seeding succeeds")
        .expect("a first run writes the file");

    assert_eq!(seeded, user.join(FILE_NAME));
    assert_eq!(
        std::fs::read_to_string(&seeded).expect("reads"),
        crate::commented_document()
    );
}

#[test]
fn an_existing_user_config_is_never_replaced() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let source = shipped(dir.path());
    let user = dir.path().join("glimpse");
    std::fs::create_dir_all(&user).expect("creates");
    std::fs::write(user.join(FILE_NAME), "[appearance]\ntheme = \"mine\"\n").expect("writes");

    assert_eq!(
        seed_from(&source, Some(&user), None).expect("succeeds"),
        None
    );
    assert_eq!(
        std::fs::read_to_string(user.join(FILE_NAME)).expect("reads"),
        "[appearance]\ntheme = \"mine\"\n"
    );
}

#[test]
fn an_explicit_config_path_seeds_nothing() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let source = shipped(dir.path());
    let user = dir.path().join("glimpse");
    let explicit = dir.path().join("elsewhere.toml");

    assert_eq!(
        seed_from(&source, Some(&user), Some(&explicit)).expect("succeeds"),
        None
    );
    assert!(!user.exists());
}

#[test]
fn a_missing_shipped_file_is_not_a_failure() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let user = dir.path().join("glimpse");

    let source = dir.path().join(COMMENTED_FILE_NAME);
    assert_eq!(
        seed_from(&source, Some(&user), None).expect("succeeds"),
        None
    );
    assert!(!user.exists());
}

#[test]
fn a_seeded_document_loads_to_the_shipped_defaults() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let source = shipped(dir.path());
    let user = dir.path().join("glimpse");
    let seeded = seed_from(&source, Some(&user), None)
        .expect("succeeds")
        .expect("writes");

    let empty = dir.path().join("empty");
    std::fs::create_dir_all(&empty).expect("creates");
    assert_eq!(
        load_from(&empty, Some(&user), None).expect("the seed loads"),
        Config::default()
    );
    assert!(seeded.is_file());
}

#[test]
fn two_binaries_starting_at_once_seed_one_intact_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let source = shipped(dir.path());
    let user = dir.path().join("glimpse");

    let written = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| seed_from(&source, Some(&user), None).expect("succeeds")))
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().expect("the thread finishes"))
            .count()
    });

    assert_eq!(written, 1);
    assert_eq!(
        std::fs::read_to_string(user.join(FILE_NAME)).expect("reads"),
        crate::commented_document()
    );
}
