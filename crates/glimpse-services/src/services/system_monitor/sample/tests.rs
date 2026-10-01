use super::*;

const STAT: &str = "cpu  28153468 1775111 9605921 167807897 82624623 658118 372139 0 0 0\ncpu0 1015079 70502 308772 5380092 5322960 22412 68151 0 0 0\n";

#[test]
fn the_aggregate_cpu_line_sums_the_first_eight_fields() {
    let (total, idle) = parse_stat(STAT).expect("the fixture has a cpu line");
    assert_eq!(
        total,
        28153468 + 1775111 + 9605921 + 167807897 + 82624623 + 658118 + 372139
    );
    assert_eq!(idle, 167807897 + 82624623);
}

#[test]
fn a_missing_cpu_line_yields_nothing() {
    assert_eq!(parse_stat("cpu0 1 2 3 4 5 6 7 8\n"), None);
}

const MEMINFO: &str = "MemTotal:       31938016 kB\nMemFree:         6400372 kB\nMemAvailable:   10527312 kB\nSwapTotal:      16777208 kB\nSwapFree:            948 kB\n";

#[test]
fn memory_and_swap_are_derived_from_total_minus_available_or_free() {
    let (memory, swap) = parse_meminfo(MEMINFO);
    let memory = memory.expect("both fields are in the fixture");
    assert_eq!(memory.total, 31938016 * 1024);
    assert_eq!(memory.used, (31938016 - 10527312) * 1024);
    let swap = swap.expect("swap total is nonzero");
    assert_eq!(swap.total, 16777208 * 1024);
    assert_eq!(swap.used, (16777208 - 948) * 1024);
}

#[test]
fn a_swap_total_of_zero_is_no_swap_at_all() {
    let (_, swap) =
        parse_meminfo("MemTotal: 1000 kB\nMemAvailable: 500 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n");
    assert_eq!(swap, None);
}

#[test]
fn loadavg_reads_the_first_three_fields() {
    let load = parse_loadavg("6.29 7.46 9.76 7/4534 3354039\n").expect("parses");
    assert_eq!(load.one, 6.29);
    assert_eq!(load.five, 7.46);
    assert_eq!(load.fifteen, 9.76);
}

#[test]
fn uptime_reads_the_first_field_as_seconds() {
    let uptime = parse_uptime("202031.70 1678079.01\n").expect("parses");
    assert_eq!(uptime, std::time::Duration::from_secs_f64(202031.70));
}

const NET_DEV: &str = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 100 1    0    0    0     0          0         0 100 1    0    0    0     0       0          0\nwlp99s0: 8973687366 11666884    0 11029    0     0          0         0 15041670643 16976278    0   35    0     0       0          0\n";

#[test]
fn every_interface_line_yields_its_rx_and_tx_byte_counters() {
    let interfaces = parse_net_dev(NET_DEV);
    assert_eq!(
        interfaces,
        vec![
            ("lo".to_owned(), 100, 100),
            ("wlp99s0".to_owned(), 8973687366, 15041670643),
        ]
    );
}

#[test]
fn a_disappearing_interface_is_simply_absent_from_the_second_parse() {
    let first = parse_net_dev(NET_DEV);
    let second = parse_net_dev(
        "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 200 2    0    0    0     0          0         0 200 2    0    0    0     0       0          0\n",
    );
    assert_eq!(first.len(), 2);
    assert_eq!(second, vec![("lo".to_owned(), 200, 200)]);
}
