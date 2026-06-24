mod types;
pub use types::*;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct CodebaseGraph {
    /// All function nodes, keyed by function name (Vec to handle same-name static functions across files)
    pub nodes: HashMap<String, Vec<FunctionNode>>,
    /// Reverse index: callee → callers with the call site line number
    pub callers: HashMap<String, Vec<CallEdge>>,
    /// Forward index: caller → callees with the call site line number
    pub callees: HashMap<String, Vec<CallEdge>>,
    /// #define constants, function-like macros, and enum values, keyed by name
    pub symbols: HashMap<String, Vec<SymbolNode>>,
    /// Type definitions: struct / union / enum / typedef, keyed by name
    pub types: HashMap<String, Vec<TypeNode>>,
    /// File-scope variable declarations, keyed by name
    pub globals: HashMap<String, Vec<GlobalVar>>,
}

impl CodebaseGraph {
    pub fn insert_node(&mut self, node: FunctionNode) {
        self.nodes.entry(node.name.clone()).or_default().push(node);
    }

    pub fn add_edge(&mut self, caller: &str, callee: &str, line: u32, caller_file: PathBuf) {
        self.callees
            .entry(caller.to_string())
            .or_default()
            .push(CallEdge { name: callee.to_string(), line, caller_file: caller_file.clone() });

        self.callers
            .entry(callee.to_string())
            .or_default()
            .push(CallEdge { name: caller.to_string(), line, caller_file });
    }

    pub fn clear(&mut self) {
        self.nodes.clear();
        self.callers.clear();
        self.callees.clear();
        self.symbols.clear();
        self.types.clear();
        self.globals.clear();
    }

    pub fn merge(&mut self, other: CodebaseGraph) {
        for (k, v) in other.nodes {
            self.nodes.entry(k).or_default().extend(v);
        }
        for (k, v) in other.callees {
            self.callees.entry(k).or_default().extend(v);
        }
        for (k, v) in other.callers {
            self.callers.entry(k).or_default().extend(v);
        }
        for (k, v) in other.symbols {
            self.symbols.entry(k).or_default().extend(v);
        }
        for (k, v) in other.types {
            self.types.entry(k).or_default().extend(v);
        }
        for (k, v) in other.globals {
            self.globals.entry(k).or_default().extend(v);
        }
    }

    pub fn insert_global(&mut self, var: GlobalVar) {
        self.globals.entry(var.name.clone()).or_default().push(var);
    }

    pub fn insert_type(&mut self, node: TypeNode) {
        self.types.entry(node.name.clone()).or_default().push(node);
    }

    pub fn insert_symbol(&mut self, node: SymbolNode) {
        self.symbols
            .entry(node.name.clone())
            .or_default()
            .push(node);
    }

    pub fn find_function(&self, query: &str) -> Vec<&FunctionNode> {
        let q = query.to_lowercase();
        self.nodes
            .values()
            .flatten()
            .filter(|n| n.name.to_lowercase().contains(&q))
            .collect()
    }

    pub fn find_functions_in_file(&self, filename: &str) -> Vec<&FunctionNode> {
        let q = filename.to_lowercase();
        self.nodes
            .values()
            .flatten()
            .filter(|n| n.file.to_string_lossy().to_lowercase().contains(&q))
            .collect()
    }

    pub fn get_callers(&self, name: &str, depth: usize) -> Vec<String> {
        self.bfs(name, depth, Direction::Callers)
    }

    pub fn get_callees(&self, name: &str, depth: usize) -> Vec<String> {
        self.bfs(name, depth, Direction::Callees)
    }

    fn bfs(&self, start: &str, depth: usize, dir: Direction) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        visited.insert(start.to_string());
        queue.push_back((start.to_string(), 0usize));

        while let Some((current, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }
            let map = match dir {
                Direction::Callers => &self.callers,
                Direction::Callees => &self.callees,
            };
            if let Some(edges) = map.get(&current) {
                for edge in edges {
                    if visited.insert(edge.name.clone()) {
                        results.push(edge.name.clone());
                        queue.push_back((edge.name.clone(), d + 1));
                    }
                }
            }
        }
        results
    }

    pub fn get_callers_from(&self, name: &str, file: &Path, depth: usize) -> Vec<(String, PathBuf)> {
        self.bfs_from(name, file, depth, Direction::Callers)
    }

    pub fn get_callees_from(&self, name: &str, file: &Path, depth: usize) -> Vec<(String, PathBuf)> {
        self.bfs_from(name, file, depth, Direction::Callees)
    }

    fn bfs_from(&self, start: &str, start_file: &Path, depth: usize, dir: Direction) -> Vec<(String, PathBuf)> {
        let mut visited: HashSet<(String, PathBuf)> = HashSet::new();
        let mut queue: VecDeque<(String, PathBuf, usize)> = VecDeque::new();
        let mut results: Vec<(String, PathBuf)> = Vec::new();

        visited.insert((start.to_string(), start_file.to_path_buf()));
        queue.push_back((start.to_string(), start_file.to_path_buf(), 0));

        while let Some((current, current_file, d)) = queue.pop_front() {
            if d >= depth {
                continue;
            }

            let map = match dir {
                Direction::Callers => &self.callers,
                Direction::Callees => &self.callees,
            };

            let is_static = self.nodes.get(&current)
                .and_then(|v| v.iter().find(|n| n.file == current_file))
                .map(|n| n.is_static)
                .unwrap_or(false);

            if let Some(edges) = map.get(&current) {
                for edge in edges {
                    let keep = match dir {
                        Direction::Callees => edge.caller_file == current_file,
                        Direction::Callers => !is_static || edge.caller_file == current_file,
                    };
                    if !keep {
                        continue;
                    }

                    let next_file = match dir {
                        Direction::Callees => self.nodes.get(&edge.name)
                            .and_then(|v| {
                                if v.len() == 1 {
                                    Some(v[0].file.clone())
                                } else {
                                    v.iter()
                                        .find(|n| n.file == current_file)
                                        .or_else(|| v.first())
                                        .map(|n| n.file.clone())
                                }
                            })
                            .unwrap_or_default(),
                        Direction::Callers => edge.caller_file.clone(),
                    };

                    let key = (edge.name.clone(), next_file.clone());
                    if visited.insert(key) {
                        results.push((edge.name.clone(), next_file.clone()));
                        queue.push_back((edge.name.clone(), next_file, d + 1));
                    }
                }
            }
        }
        results
    }
}

enum Direction {
    Callers,
    Callees,
}

/// Merge a collection of per-file file graphs into a single codebase graph.
pub fn merge_file_graphs(file_graphs: Vec<FileGraph>) -> CodebaseGraph {
    let mut g = CodebaseGraph::default();

    for fg in file_graphs {
        for (k, v) in fg.nodes {
            g.nodes.entry(k).or_default().extend(v);
        }
        for (caller, edges) in fg.callees {
            for edge in &edges {
                g.callers
                    .entry(edge.name.clone())
                    .or_default()
                    .push(CallEdge { name: caller.clone(), line: edge.line, caller_file: edge.caller_file.clone() });
            }
            g.callees.entry(caller).or_default().extend(edges);
        }
        for (k, v) in fg.symbols {
            g.symbols.entry(k).or_default().extend(v);
        }
        for (k, v) in fg.types {
            g.types.entry(k).or_default().extend(v);
        }
        for (k, v) in fg.globals {
            g.globals.entry(k).or_default().extend(v);
        }
    }

    g
}
