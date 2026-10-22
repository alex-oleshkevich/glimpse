use std::future::Future;
use std::io::Cursor;

use futures_util::StreamExt;
use tokio::io::BufReader;

use super::*;
use crate::model::WindowId;

struct Numbers;

impl LineDecoder for Numbers {
    type Event = Event;

    fn decode(&mut self, line: &str) -> impl Future<Output = Vec<Event>> + Send {
        let events = line
            .split(',')
            .filter_map(|part| part.parse::<u64>().ok())
            .map(|id| Event::WindowClosed(WindowId(id)))
            .collect();
        std::future::ready(events)
    }
}

async fn events_from(bytes: &[u8]) -> Vec<Event> {
    line_events(BufReader::new(Cursor::new(bytes.to_vec())), Numbers)
        .collect()
        .await
}

#[tokio::test]
async fn a_final_line_with_no_newline_is_delivered_before_the_stream_ends() {
    let events = events_from(b"9\r\n4").await;

    assert_eq!(
        events,
        [
            Event::WindowClosed(WindowId(9)),
            Event::WindowClosed(WindowId(4)),
        ]
    );
}

#[tokio::test]
async fn an_empty_stream_ends_with_no_events() {
    assert_eq!(events_from(b"").await, Vec::new());
}

#[tokio::test]
async fn a_batch_from_one_line_is_yielded_in_order_before_the_next_line() {
    let events = events_from(b"9,4\n7\n").await;

    assert_eq!(
        events,
        [
            Event::WindowClosed(WindowId(9)),
            Event::WindowClosed(WindowId(4)),
            Event::WindowClosed(WindowId(7)),
        ]
    );
}

#[tokio::test]
async fn a_non_utf8_byte_is_replaced_and_the_stream_survives() {
    let events = events_from(b"a title with a bad \xff byte\n9\n").await;

    assert_eq!(events, [Event::WindowClosed(WindowId(9))]);
}
