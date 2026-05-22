use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

pub struct FunctionNode {
    pub name: String,
    pub file: PathBuf,
    pub line: u32,
    pub source: String,
}

#[derive(Default)]
pub struct CallGraph {
    pub nodes: HashMap<String, FunctionNode>,
    pub callers: HashMap<String, HashSet<String>>,
    pub callees: HashMap<String, HashSet<String>>,
}

impl CallGraph {
    pub fn insert_node(&mut self, node: FunctionNode) {
        self.nodes.insert(node.name.clone(), node);
    }

    pub fn add_edge(&mut self, caller: &str, callee: &str) {
        self.callees
            .entry(caller.to_string())
            .or_default()
            .insert(callee.to_string());
        self.callers
            .entry(callee.to_string())
            .or_default()
            .insert(caller.to_string());
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.callers.clear();
        self.callees.clear();
    }

    pub fn get_callers(&self, name: &str, depth: usize) -> Vec<String> {
        self.bfs(name, depth, &self.callers)
    }

    pub fn get_callees(&self, name: &str, depth: usize) -> Vec<String> {
        self.bfs(name, depth, &self.callees)
    }

    fn bfs(
        &self,
        start: &str,
        depth: usize,
        map: &HashMap<String, HashSet<String>>,
    ) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        visited.insert(start.to_string());
        queue.push_back((start.to_string(), 0usize));

        while let Some((current, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            if let Some(neighbors) = map.get(&current) {
                for neighbor in neighbors {
                    if visited.insert(neighbor.clone()) {
                        results.push(neighbor.clone());
                        queue.push_back((neighbor.clone(), d + 1));
                    }
                }
            }
        }
        results
    }

    pub fn find_function(&self, query: &str) -> Vec<&FunctionNode> {
        let q = query.to_lowercase();
        self.nodes
            .values()
            .filter(|n| n.name.to_lowercase().contains(&q))
            .collect()
    }
}
