use super::*;

#[test]
fn every_hundredth_pixel_reaches_across_the_track_and_every_fiftieth_halfway() {
    assert_eq!(reach(0), 1.0);
    assert_eq!(reach(300), 1.0);
    assert_eq!(reach(250), 0.5);
    assert_eq!(reach(260), 0.25);
}
