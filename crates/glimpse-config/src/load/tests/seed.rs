use super::*;

const CONFIG: &str = concat!(
    "#:schema ",
    env!("GLIMPSE_DATA_DIR_DEFAULT"),
    "/config.schema.json\n# https://alex-oleshkevich.github.io/glimpse/config/\n"
);
const PANEL: &str = concat!(
    "#:schema ",
    env!("GLIMPSE_DATA_DIR_DEFAULT"),
    "/panel.schema.json\n# https://alex-oleshkevich.github.io/glimpse/config/panels\n"
);

#[test]
fn a_first_run_seeds_both_stubs_byte_for_byte() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let user = dir.path().join("config/glimpse");

    let created = seed_from(None, Some(&user)).expect("seeding succeeds");

    assert_eq!(created, [user.join("config.toml"), user.join("panel.toml")]);
    assert_eq!(fs::read_to_string(&created[0]).expect("reads"), CONFIG);
    assert_eq!(fs::read_to_string(&created[1]).expect("reads"), PANEL);
}

#[test]
fn an_existing_file_is_never_replaced_and_the_other_is_still_seeded() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let user = dir.path().join("glimpse");
    write(
        &user.join("config.toml"),
        "[appearance]\ntheme = \"mine\"\n",
    );

    let created = seed_from(None, Some(&user)).expect("succeeds");

    assert_eq!(created, [user.join("panel.toml")]);
    assert_eq!(
        fs::read_to_string(user.join("config.toml")).expect("reads"),
        "[appearance]\ntheme = \"mine\"\n"
    );
}

#[test]
fn a_config_dir_seeds_nothing() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let user = dir.path().join("glimpse");
    let explicit = dir.path().join("elsewhere");

    assert!(
        seed_from(Some(&explicit), Some(&user))
            .expect("succeeds")
            .is_empty()
    );
    assert!(!user.exists());
    assert!(!explicit.exists());
}

#[test]
fn no_user_dir_seeds_nothing() {
    assert!(seed_from(None, None).expect("succeeds").is_empty());
}

#[test]
fn the_seeded_stubs_load_to_the_shipped_defaults() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let user = dir.path().join("glimpse");
    let created = seed_from(None, Some(&user)).expect("succeeds");

    assert_eq!(
        load_files(&created).expect("the stubs load"),
        Config::default()
    );
}

#[test]
fn two_binaries_starting_at_once_seed_each_file_once() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let user = dir.path().join("glimpse");

    let written = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| seed_from(None, Some(&user)).expect("succeeds")))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("the thread finishes").len())
            .sum::<usize>()
    });

    assert_eq!(written, 2);
    assert_eq!(
        fs::read_to_string(user.join("config.toml")).expect("reads"),
        CONFIG
    );
    assert_eq!(
        fs::read_to_string(user.join("panel.toml")).expect("reads"),
        PANEL
    );
}
