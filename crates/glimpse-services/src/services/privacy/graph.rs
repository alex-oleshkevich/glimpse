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
mod tests;
