use async_trait::async_trait;
use glimpse_sdk::{Applet, AppletResult, InitEvent, StatusItem, TreeNode, run};

mod config;

use config::Config;

/// Held by the `Applet` trait impl; the SDK clones this between ticks.
/// At this stage the applet is a no-op skeleton: it parses its config out
/// of the init event so a config error surfaces immediately, then emits an
/// empty status list. Sampler-driven indicators land in the next task.
#[derive(Debug, Clone, Default)]
struct State {
    config: Config,
}

/// Reserved for future inbound messages (sampler ticks, popover events,
/// kill/terminate requests). The skeleton has no producers yet, hence
/// uninhabited via the empty enum.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Msg {}

struct SysmonitorApplet;

#[async_trait]
impl Applet for SysmonitorApplet {
    type State = State;
    type Msg = Msg;

    async fn on_init(&mut self, state: &mut State, event: InitEvent) -> AppletResult<()> {
        match serde_json::from_value::<Config>(event.options.clone()) {
            Ok(config) => {
                state.config = config;
            }
            Err(error) => {
                // Soft-fail: log and continue with defaults. A panic here would
                // crash-loop the supervisor since exec applets restart on exit.
                eprintln!("sysmonitor: invalid applet config: {error}");
            }
        }
        Ok(())
    }

    async fn status(&self, _state: &State) -> AppletResult<Vec<StatusItem>> {
        Ok(Vec::new())
    }

    async fn popover(&self, _state: &State) -> AppletResult<Option<TreeNode<Msg>>> {
        Ok(None)
    }
}

#[tokio::main]
async fn main() -> glimpse_sdk::AppletResult<()> {
    run(SysmonitorApplet, State::default()).await
}
