use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glimpse_services::{CalendarState, FailingSource};
use tokio::sync::watch;

use super::follow;

#[derive(Default)]
struct Record {
    posts: Vec<(u32, String, String)>,
    closes: Vec<u32>,
    refuse_close: bool,
}

struct Notes(Arc<Mutex<Record>>);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notes {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app_name: &str,
        replaces_id: u32,
        _app_icon: &str,
        summary: &str,
        body: &str,
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        self.0
            .lock()
            .expect("record")
            .posts
            .push((replaces_id, summary.into(), body.into()));
        7
    }

    fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        let mut record = self.0.lock().expect("record");
        record.closes.push(id);
        if record.refuse_close {
            return Err(zbus::fdo::Error::Failed("no such notification".into()));
        }
        Ok(())
    }
}

fn source(id: &str, reason: &str) -> FailingSource {
    FailingSource {
        id: id.into(),
        name: format!("name {id}"),
        reason: reason.into(),
    }
}

fn state(failing: Vec<FailingSource>) -> CalendarState {
    CalendarState {
        failing,
        ..CalendarState::default()
    }
}

async fn until(record: &Arc<Mutex<Record>>, ready: impl Fn(&Record) -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !ready(&record.lock().expect("record")) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("recorded call");
}

async fn serve(
    record: &Arc<Mutex<Record>>,
) -> (
    glimpse_dbus::testing::PrivateBus,
    zbus::Connection,
    watch::Sender<CalendarState>,
    tokio::task::JoinHandle<()>,
) {
    let bus = glimpse_dbus::testing::PrivateBus::start();
    let server = bus.connection().await;
    server
        .object_server()
        .at("/org/freedesktop/Notifications", Notes(record.clone()))
        .await
        .expect("notes");
    glimpse_dbus::own_name(&server, "org.freedesktop.Notifications")
        .await
        .expect("notification name");
    let (sender, receiver) = watch::channel(CalendarState::default());
    let task = tokio::spawn(follow(bus.connection().await, receiver));
    (bus, server, sender, task)
}

#[tokio::test]
async fn failing_sources_are_posted_replaced_and_closed() {
    let record = Arc::new(Mutex::new(Record::default()));
    let (_bus, _server, sender, task) = serve(&record).await;

    sender
        .send(state(vec![source("a", "<b>& unreadable")]))
        .expect("send");
    until(&record, |record| record.posts.len() == 1).await;
    {
        let record = record.lock().expect("record");
        assert_eq!(record.posts[0].0, 0);
        assert_eq!(record.posts[0].1, "1 calendar cannot be read");
        assert!(record.posts[0].2.contains("&lt;b&gt;&amp; unreadable"));
        assert!(!record.posts[0].2.contains("<b>"));
    }

    sender
        .send(state(vec![source("a", "another reason")]))
        .expect("send");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(record.lock().expect("record").posts.len(), 1);
    sender
        .send(state(vec![
            source("a", "another reason"),
            source("b", "gone"),
        ]))
        .expect("send");
    until(&record, |record| record.posts.len() == 2).await;
    {
        let record = record.lock().expect("record");
        assert_eq!(record.posts.len(), 2);
        assert_eq!(record.posts[1].0, 7);
        assert_eq!(record.posts[1].1, "2 calendars cannot be read");
        assert_eq!(record.posts[1].2, "name a: another reason\nname b: gone");
    }

    sender.send(state(Vec::new())).expect("send");
    until(&record, |record| !record.closes.is_empty()).await;
    {
        let record = record.lock().expect("record");
        assert_eq!(record.closes, [7]);
        assert_eq!(record.posts.len(), 2);
    }

    task.abort();
}

#[tokio::test]
async fn a_refused_close_does_not_swallow_the_next_failure() {
    let record = Arc::new(Mutex::new(Record {
        refuse_close: true,
        ..Record::default()
    }));
    let (_bus, _server, sender, task) = serve(&record).await;

    sender.send(state(vec![source("a", "gone")])).expect("send");
    until(&record, |record| record.posts.len() == 1).await;
    sender.send(state(Vec::new())).expect("send");
    until(&record, |record| record.closes == [7]).await;
    sender.send(state(vec![source("a", "gone")])).expect("send");
    until(&record, |record| record.posts.len() == 2).await;
    assert_eq!(record.lock().expect("record").posts[1].0, 0);

    task.abort();
}
