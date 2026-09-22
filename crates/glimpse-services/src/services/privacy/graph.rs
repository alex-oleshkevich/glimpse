use std::collections::HashMap;

const NODE_NAME_KEY: &str = "node.name";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkRecord {
    pub output_node_id: u32,
    pub input_node_id: u32,
    pub active: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Graph {
    nodes: HashMap<u32, HashMap<String, String>>,
    links: HashMap<u32, LinkRecord>,
}

impl Graph {
    pub fn seed_node(&mut self, id: u32, props: HashMap<String, String>) {
        self.nodes.insert(id, props);
    }

    pub fn merge_node(&mut self, id: u32, props: HashMap<String, String>) {
        self.nodes.entry(id).or_default().extend(props);
    }

    pub fn remove_node(&mut self, id: u32) {
        self.nodes.remove(&id);
    }

    pub fn set_link(&mut self, id: u32, record: LinkRecord) {
        self.links.insert(id, record);
    }

    pub fn remove_link(&mut self, id: u32) {
        self.links.remove(&id);
    }

    pub fn attribution(&self) -> HashMap<u32, String> {
        let mut resolved = HashMap::new();
        for link in self.links.values().filter(|link| link.active) {
            if let Some(name) = self.node_name(link.input_node_id) {
                resolved.insert(link.output_node_id, name.clone());
            }
            if let Some(name) = self.node_name(link.output_node_id) {
                resolved.insert(link.input_node_id, name.clone());
            }
        }
        resolved
    }

    fn node_name(&self, id: u32) -> Option<&String> {
        self.nodes.get(&id)?.get(NODE_NAME_KEY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(name: &str) -> HashMap<String, String> {
        HashMap::from([(NODE_NAME_KEY.to_owned(), name.to_owned())])
    }

    #[test]
    fn an_active_link_names_the_producer_from_the_consumer_node_name() {
        let mut graph = Graph::default();
        graph.seed_node(107, HashMap::new());
        graph.seed_node(110, props("chrome"));
        graph.set_link(
            1,
            LinkRecord {
                output_node_id: 107,
                input_node_id: 110,
                active: true,
            },
        );

        let attribution = graph.attribution();
        assert_eq!(attribution.get(&107).map(String::as_str), Some("chrome"));
    }

    #[test]
    fn link_orientation_is_matched_on_either_end() {
        let mut graph = Graph::default();
        graph.seed_node(107, props("chrome"));
        graph.seed_node(110, HashMap::new());
        graph.set_link(
            1,
            LinkRecord {
                output_node_id: 110,
                input_node_id: 107,
                active: true,
            },
        );

        let attribution = graph.attribution();
        assert_eq!(
            attribution.get(&110).map(String::as_str),
            Some("chrome"),
            "the cast's node can be either end of the link, so the producer named 110 here must \
             still resolve"
        );
    }

    #[test]
    fn an_inactive_link_names_nobody() {
        let mut graph = Graph::default();
        graph.seed_node(107, HashMap::new());
        graph.seed_node(110, props("chrome"));
        graph.set_link(
            1,
            LinkRecord {
                output_node_id: 107,
                input_node_id: 110,
                active: false,
            },
        );

        assert!(graph.attribution().is_empty());
    }

    #[test]
    fn an_empty_info_event_never_wipes_a_seeded_name() {
        let mut graph = Graph::default();
        graph.seed_node(110, props("chrome"));

        graph.merge_node(110, HashMap::new());

        assert_eq!(
            graph.attribution_source_name(110),
            Some("chrome"),
            "a stream node's info event alternates between full and empty on consecutive \
             updates; an empty one must merge, never replace"
        );
    }

    #[test]
    fn a_later_info_event_merges_new_fields_in_rather_than_replacing_the_node() {
        let mut graph = Graph::default();
        graph.seed_node(
            110,
            HashMap::from([("media.class".to_owned(), "Stream/Input/Video".to_owned())]),
        );

        graph.merge_node(110, props("chrome"));

        assert_eq!(graph.attribution_source_name(110), Some("chrome"));
    }

    #[test]
    fn removing_a_node_or_link_drops_its_attribution() {
        let mut graph = Graph::default();
        graph.seed_node(107, HashMap::new());
        graph.seed_node(110, props("chrome"));
        graph.set_link(
            1,
            LinkRecord {
                output_node_id: 107,
                input_node_id: 110,
                active: true,
            },
        );
        assert!(!graph.attribution().is_empty());

        graph.remove_link(1);
        assert!(graph.attribution().is_empty());
    }

    impl Graph {
        fn attribution_source_name(&self, id: u32) -> Option<&str> {
            self.nodes.get(&id)?.get(NODE_NAME_KEY).map(String::as_str)
        }
    }
}
