use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use futures_util::{Stream, stream};
use pipewire::context::ContextRc;
use pipewire::core::PW_ID_CORE;
use pipewire::link::{Link, LinkListener, LinkState};
use pipewire::main_loop::MainLoopRc;
use pipewire::node::{Node, NodeListener};
use pipewire::registry::{GlobalObject, RegistryRc};
use pipewire::spa::utils::dict::DictRef;
use pipewire::types::ObjectType;
use tokio::sync::mpsc;

use super::Event;
use super::graph::{Graph, LinkRecord};

const RETRY_DELAY: Duration = Duration::from_secs(5);

struct Terminate;

type BoundNodes = Rc<RefCell<HashMap<u32, (Node, NodeListener)>>>;
type BoundLinks = Rc<RefCell<HashMap<u32, (Link, LinkListener)>>>;

pub async fn attribution() -> impl Stream<Item = Event> + Send + 'static {
    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let report_tx = events_tx.clone();
    let (pw_sender, pw_receiver) = pipewire::channel::channel::<Terminate>();

    if thread::Builder::new()
        .name("glimpse-pipewire".to_owned())
        .spawn(move || bridge_main(pw_receiver, events_tx))
        .is_err()
    {
        let _ = report_tx.send(Event::Graph(HashMap::new()));
    }

    let guard = TerminateOnDrop(Some(pw_sender));
    stream::unfold((events_rx, guard), |(mut rx, guard)| async move {
        rx.recv().await.map(|event| (event, (rx, guard)))
    })
}

struct TerminateOnDrop(Option<pipewire::channel::Sender<Terminate>>);

impl Drop for TerminateOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(Terminate);
        }
    }
}

fn bridge_main(receiver: pipewire::channel::Receiver<Terminate>, tx: mpsc::UnboundedSender<Event>) {
    let mut receiver = receiver;
    loop {
        let (next_receiver, stopped) = run_once(receiver, &tx);
        receiver = next_receiver;
        if stopped {
            break;
        }
        if tx.send(Event::Graph(HashMap::new())).is_err() {
            break;
        }
        thread::sleep(RETRY_DELAY);
    }
}

fn run_once(
    receiver: pipewire::channel::Receiver<Terminate>,
    tx: &mpsc::UnboundedSender<Event>,
) -> (pipewire::channel::Receiver<Terminate>, bool) {
    let Ok(mainloop) = MainLoopRc::new(None) else {
        return (receiver, false);
    };
    let Ok(context) = ContextRc::new(&mainloop, None) else {
        return (receiver, false);
    };
    let Ok(core) = context.connect_rc(None) else {
        return (receiver, false);
    };
    let Ok(registry) = core.get_registry_rc() else {
        return (receiver, false);
    };

    let stopped = Rc::new(Cell::new(false));
    let attached = {
        let quit_loop = mainloop.clone();
        let stopped = Rc::clone(&stopped);
        receiver.attach(mainloop.loop_(), move |Terminate| {
            stopped.set(true);
            quit_loop.quit();
        })
    };

    let _core_listener = {
        let mainloop = mainloop.clone();
        core.add_listener_local()
            .error(move |id, _seq, _res, _message| {
                if id == PW_ID_CORE {
                    mainloop.quit();
                }
            })
            .register()
    };

    let graph = Rc::new(RefCell::new(Graph::default()));
    let bound_nodes: BoundNodes = Rc::new(RefCell::new(HashMap::new()));
    let bound_links: BoundLinks = Rc::new(RefCell::new(HashMap::new()));

    let _registry_listener = {
        let listener_registry = registry.clone();
        let graph = Rc::clone(&graph);
        let bound_nodes = Rc::clone(&bound_nodes);
        let bound_links = Rc::clone(&bound_links);
        let global_tx = tx.clone();
        let remove_tx = tx.clone();
        let remove_graph = Rc::clone(&graph);
        let remove_nodes = Rc::clone(&bound_nodes);
        let remove_links = Rc::clone(&bound_links);
        registry
            .add_listener_local()
            .global(move |global| {
                on_global(
                    &listener_registry,
                    global,
                    &graph,
                    &bound_nodes,
                    &bound_links,
                    &global_tx,
                );
            })
            .global_remove(move |id| {
                remove_nodes.borrow_mut().remove(&id);
                remove_links.borrow_mut().remove(&id);
                let mut current = remove_graph.borrow_mut();
                current.remove_node(id);
                current.remove_link(id);
                let _ = remove_tx.send(Event::Graph(current.attribution()));
            })
            .register()
    };

    mainloop.run();

    (attached.deattach(), stopped.get())
}

fn on_global(
    registry: &RegistryRc,
    global: &GlobalObject<&DictRef>,
    graph: &Rc<RefCell<Graph>>,
    bound_nodes: &BoundNodes,
    bound_links: &BoundLinks,
    tx: &mpsc::UnboundedSender<Event>,
) {
    match global.type_ {
        ObjectType::Node => {
            graph
                .borrow_mut()
                .seed_node(global.id, dict_map(global.props));
            let _ = tx.send(Event::Graph(graph.borrow().attribution()));

            let Ok(node): Result<Node, _> = registry.bind(global) else {
                return;
            };
            let id = global.id;
            let node_graph = Rc::clone(graph);
            let node_tx = tx.clone();
            let listener = node
                .add_listener_local()
                .info(move |info| {
                    let props = dict_map(info.props());
                    node_graph.borrow_mut().merge_node(id, props);
                    let _ = node_tx.send(Event::Graph(node_graph.borrow().attribution()));
                })
                .register();
            bound_nodes.borrow_mut().insert(id, (node, listener));
        }
        ObjectType::Link => {
            let Ok(link): Result<Link, _> = registry.bind(global) else {
                return;
            };
            let id = global.id;
            let link_graph = Rc::clone(graph);
            let link_tx = tx.clone();
            let listener = link
                .add_listener_local()
                .info(move |info| {
                    let record = LinkRecord {
                        output_node_id: info.output_node_id(),
                        input_node_id: info.input_node_id(),
                        active: matches!(info.state(), LinkState::Active),
                    };
                    link_graph.borrow_mut().set_link(id, record);
                    let _ = link_tx.send(Event::Graph(link_graph.borrow().attribution()));
                })
                .register();
            bound_links.borrow_mut().insert(id, (link, listener));
        }
        _ => {}
    }
}

fn dict_map(dict: Option<&DictRef>) -> HashMap<String, String> {
    dict.map(|dict| {
        dict.iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect()
    })
    .unwrap_or_default()
}
