use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct FunctionNode {
    /// Fully qualified function name
    pub name: String,
    /// Path to the source file where the function is defined
    pub file: PathBuf,
    /// Line number of the function definition
    pub line: u32,
    /// Raw source code of the function
    pub source: String,
}

#[derive(Default, Serialize, Deserialize)]
pub struct CallGraph {
    /// All function nodes, keyed by function name
    pub nodes: HashMap<String, FunctionNode>,
    /// Reverse index: key is the callee, value is the set of functions that call it
    pub callers: HashMap<String, HashSet<String>>,
    /// Forward index: key is the caller, value is the set of functions it calls
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

    /// BFS from `from` through callees; returns the shortest call path to `to`, or None.
    pub fn find_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        if from == to {
            return Some(vec![from.to_string()]);
        }
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        visited.insert(from.to_string());
        queue.push_back(from.to_string());

        while let Some(current) = queue.pop_front() {
            if let Some(callees) = self.callees.get(&current) {
                for callee in callees {
                    if visited.insert(callee.clone()) {
                        parent.insert(callee.clone(), current.clone());
                        if callee == to {
                            let mut path = vec![to.to_string()];
                            let mut node = to.to_string();
                            while let Some(p) = parent.get(&node) {
                                path.push(p.clone());
                                node = p.clone();
                            }
                            path.reverse();
                            return Some(path);
                        }
                        queue.push_back(callee.clone());
                    }
                }
            }
        }
        None
    }

    /// Returns all functions that have no callers (potential dead code).
    pub fn find_dead_code(&self) -> Vec<&FunctionNode> {
        self.nodes
            .values()
            .filter(|n| self.callers.get(&n.name).map_or(true, |s| s.is_empty()))
            .collect()
    }
}
