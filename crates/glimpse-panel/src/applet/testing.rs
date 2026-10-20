use std::future::Future;
use std::pin::pin;
use std::time::Duration;

use gtk4::glib;

pub(crate) fn run_in<R>(context: &glib::MainContext, work: impl FnOnce() -> R) -> R {
    context
        .with_thread_default(work)
        .expect("the private main context is free")
}

pub(crate) async fn driven<T>(context: &glib::MainContext, wanted: impl Future<Output = T>) -> T {
    let _owner = context.acquire().expect("the private main context is free");
    let mut wanted = pin!(wanted);
    for _ in 0..500 {
        while context.iteration(false) {}
        if let Ok(value) = tokio::time::timeout(Duration::from_millis(10), &mut wanted).await {
            return value;
        }
    }
    panic!("nothing arrived while the main context ran");
}
