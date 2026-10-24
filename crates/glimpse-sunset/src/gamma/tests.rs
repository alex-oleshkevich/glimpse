use super::*;

#[test]
fn one_worker_keeps_running_while_block_in_place_waits() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("a runtime");
    let (tx, rx) = std::sync::mpsc::channel();
    runtime.spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        tx.send(()).expect("the channel stays open");
    });
    runtime.block_on(async {
        tokio::task::block_in_place(|| std::thread::sleep(std::time::Duration::from_millis(200)));
    });
    rx.recv_timeout(std::time::Duration::from_millis(500))
        .expect("the other task ran while block_in_place held the first worker");
}

#[test]
fn daylight_is_neutral_on_every_channel() {
    let (red, green, blue) = scales(6500);

    for channel in [red, green, blue] {
        assert!(
            (channel - 1.0).abs() < 1e-6,
            "{channel} is not neutral; a channel below one tints the screen whenever nothing \
                 is meant to be applied"
        );
    }
}

#[test]
fn a_warmer_temperature_keeps_red_and_cuts_blue() {
    let (red, green, blue) = scales(3000);

    assert_eq!(red, 1.0);
    assert!(blue < green && green < red, "{red} {green} {blue}");
}

#[test]
fn an_absurd_temperature_is_clamped_rather_than_refused() {
    for kelvin in [0, 1, u32::MAX] {
        let (red, green, blue) = scales(kelvin);
        for channel in [red, green, blue] {
            assert!((0.0..=1.0).contains(&channel), "{channel} is out of range");
        }
    }
}

#[test]
fn a_ramp_is_three_u16_channels_of_the_declared_size() {
    let size = 256;

    let file = ramp(size, 1.0, 1.0, 1.0).expect("a memfd");

    assert_eq!(
        file.metadata().expect("a size").len(),
        u64::from(size) * 3 * size_of::<u16>() as u64
    );
}

#[test]
fn a_ramp_rises_to_full_scale_at_its_last_step() {
    use std::io::Read as _;
    let size = 4;

    let mut file = ramp(size, 1.0, 1.0, 1.0).expect("a memfd");
    let mut table = Vec::new();
    file.read_to_end(&mut table).expect("readable");

    let last = u16::from_ne_bytes([table[6], table[7]]);
    assert_eq!(last, u16::MAX, "the red channel ends at full scale");
}
