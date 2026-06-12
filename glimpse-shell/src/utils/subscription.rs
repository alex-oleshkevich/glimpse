use relm4::Sender;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub fn subscribe_service<S, Msg>(
    mut rx: watch::Receiver<S>,
    sender: Sender<Msg>,
    map: impl Fn(S) -> Msg + Send + 'static,
) -> CancellationToken
where
    S: Clone + Send + Sync + 'static,
    Msg: Send + 'static,
{
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    relm4::spawn(async move {
        if sender.send(map(rx.borrow().clone())).is_err() {
            return;
        }
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                result = rx.changed() => {
                    if result.is_err() || sender.send(map(rx.borrow().clone())).is_err() {
                        break;
                    }
                }
            }
        }
    });
    token
}
