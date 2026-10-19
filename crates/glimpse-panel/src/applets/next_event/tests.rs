use super::*;

#[test]
fn the_list_never_reaches_less_far_than_the_bar() {
    let settings = NextEventConfig {
        within: 180,
        horizon: 60,
        ..NextEventConfig::default()
    };

    assert_eq!(horizon(&settings), render::window(settings.within));
    assert_eq!(horizon(&settings), TimeDelta::minutes(180));
}
