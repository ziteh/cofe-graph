use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum DeadCodeKind {
    /// Appears as an argument to a macro call (e.g. NRF_SDH_BLE_OBSERVER(..., my_handler, ...))
    MacroRegistered,
    /// Name matches common callback/handler naming conventions
    CallbackByName,
    /// Known entrypoint (main, etc.)
    Entrypoint,
    /// No evidence of use — genuinely suspicious
    Suspicious,
}

impl DeadCodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            DeadCodeKind::MacroRegistered => "macro_registered",
            DeadCodeKind::CallbackByName => "callback_by_name",
            DeadCodeKind::Entrypoint => "entrypoint",
            DeadCodeKind::Suspicious => "suspicious",
        }
    }
}

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
    /// true if the function has `static` storage class (file-private)
    pub is_static: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Define,
    MacroFn,
    EnumValue,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::Define => "define",
            SymbolKind::MacroFn => "macro_fn",
            SymbolKind::EnumValue => "enum_value",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum TypeKind {
    Struct,
    Union,
    Enum,
    Typedef,
}

impl TypeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TypeKind::Struct => "struct",
            TypeKind::Union => "union",
            TypeKind::Enum => "enum",
            TypeKind::Typedef => "typedef",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TypeNode {
    pub name: String,
    pub kind: TypeKind,
    /// Raw source of the full definition, trimmed to 500 chars
    pub definition: String,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IncludeEdge {
    /// Raw path as written in source (e.g. "../config/sdk_config.h" or "nrf_sdh.h")
    pub path: String,
    /// true for <system.h>, false for "local.h"
    pub is_system: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SymbolNode {
    pub name: String,
    pub kind: SymbolKind,
    /// The value/body text, trimmed to 200 chars
    pub value: Option<String>,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Default, Serialize, Deserialize)]
pub struct CallGraph {
    /// All function nodes, keyed by function name
    pub nodes: HashMap<String, FunctionNode>,
    /// Reverse index: key is the callee, value is the set of functions that call it
    pub callers: HashMap<String, HashSet<String>>,
    /// Forward index: key is the caller, value is the set of functions it calls
    pub callees: HashMap<String, HashSet<String>>,
    /// Functions that appear as arguments to macro calls (UPPER_CASE identifiers)
    pub macro_referenced: HashSet<String>,
    /// #define constants, function-like macros, and enum values, keyed by name
    pub symbols: HashMap<String, Vec<SymbolNode>>,
    /// Include edges: file path → list of #include directives in that file
    pub includes: HashMap<PathBuf, Vec<IncludeEdge>>,
    /// Type definitions: struct / union / enum / typedef, keyed by name
    pub types: HashMap<String, Vec<TypeNode>>,
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
        self.macro_referenced.clear();
        self.symbols.clear();
        self.includes.clear();
        self.types.clear();
    }

    pub fn insert_type(&mut self, node: TypeNode) {
        self.types.entry(node.name.clone()).or_default().push(node);
    }

    pub fn find_type(&self, query: &str) -> Vec<&TypeNode> {
        let q = query.to_lowercase();
        let mut results: Vec<&TypeNode> = self
            .types
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&q))
            .flat_map(|(_, nodes)| nodes.iter())
            .collect();
        results.sort_by(|a, b| a.name.cmp(&b.name).then(a.file.cmp(&b.file)));
        results
    }

    /// Returns functions whose source text contains `type_name` as a whole word.
    pub fn get_type_users(&self, type_name: &str) -> Vec<&FunctionNode> {
        let mut results: Vec<&FunctionNode> = self
            .nodes
            .values()
            .filter(|n| contains_word(&n.source, type_name))
            .collect();
        results.sort_by_key(|n| &n.name);
        results
    }

    pub fn insert_symbol(&mut self, node: SymbolNode) {
        self.symbols
            .entry(node.name.clone())
            .or_default()
            .push(node);
    }

    /// Returns all #include edges for files whose path contains `filename`.
    pub fn get_includes(&self, filename: &str) -> Vec<(&PathBuf, &Vec<IncludeEdge>)> {
        let q = filename.to_lowercase();
        let mut results: Vec<(&PathBuf, &Vec<IncludeEdge>)> = self
            .includes
            .iter()
            .filter(|(p, _)| p.to_string_lossy().to_lowercase().contains(&q))
            .collect();
        results.sort_by_key(|(p, _)| p.as_path());
        results
    }

    /// Returns all files that include a header matching `header` (substring of the raw include path).
    pub fn get_includers(&self, header: &str) -> Vec<(&PathBuf, &IncludeEdge)> {
        let q = header.to_lowercase();
        let mut results: Vec<(&PathBuf, &IncludeEdge)> = self
            .includes
            .iter()
            .flat_map(|(file, edges)| {
                edges
                    .iter()
                    .filter(|e| e.path.to_lowercase().contains(&q))
                    .map(move |e| (file, e))
            })
            .collect();
        results.sort_by_key(|(p, _)| p.as_path());
        results
    }

    pub fn find_symbol(&self, query: &str) -> Vec<&SymbolNode> {
        let q = query.to_lowercase();
        let mut results: Vec<&SymbolNode> = self
            .symbols
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&q))
            .flat_map(|(_, nodes)| nodes.iter())
            .collect();
        results.sort_by(|a, b| a.name.cmp(&b.name).then(a.file.cmp(&b.file)));
        results
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

    fn classify_dead(&self, name: &str) -> DeadCodeKind {
        if name == "main" {
            return DeadCodeKind::Entrypoint;
        }
        if self.macro_referenced.contains(name) {
            return DeadCodeKind::MacroRegistered;
        }
        let lower = name.to_lowercase();
        let suffixes = [
            "_handler",
            "_callback",
            "_cb",
            "_isr",
            "_irq",
            "_evt",
            "_event",
        ];
        let prefixes = ["on_", "assert_"];
        if suffixes.iter().any(|s| lower.ends_with(s))
            || prefixes.iter().any(|p| lower.starts_with(p))
        {
            return DeadCodeKind::CallbackByName;
        }
        DeadCodeKind::Suspicious
    }

    /// Returns all functions that have no callers, with a classification of why.
    pub fn find_dead_code(&self) -> Vec<(&FunctionNode, DeadCodeKind)> {
        self.nodes
            .values()
            .filter(|n| self.callers.get(&n.name).map_or(true, |s| s.is_empty()))
            .map(|n| {
                let kind = self.classify_dead(&n.name);
                (n, kind)
            })
            .collect()
    }

    /// Returns all functions defined in files whose path contains `filename`.
    pub fn find_functions_in_file<'a>(&'a self, filename: &str) -> Vec<&'a FunctionNode> {
        let q = filename.to_lowercase();
        self.nodes
            .values()
            .filter(|n| n.file.to_string_lossy().to_lowercase().contains(&q))
            .collect()
    }

    /// Returns non-static functions in files matching `filename` — the public API surface.
    pub fn get_public_api<'a>(&'a self, filename: &str) -> Vec<&'a FunctionNode> {
        let q = filename.to_lowercase();
        self.nodes
            .values()
            .filter(|n| !n.is_static && n.file.to_string_lossy().to_lowercase().contains(&q))
            .collect()
    }

    /// Returns the top `n` functions by fan-in (number of distinct callers).
    pub fn top_by_fan_in(&self, n: usize) -> Vec<(&str, usize)> {
        let mut ranked: Vec<(&str, usize)> = self
            .nodes
            .keys()
            .map(|name| {
                let count = self.callers.get(name).map_or(0, |s| s.len());
                (name.as_str(), count)
            })
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        ranked.truncate(n);
        ranked
    }

    /// Returns the top `n` functions by fan-out (number of distinct callees).
    pub fn top_by_fan_out(&self, n: usize) -> Vec<(&str, usize)> {
        let mut ranked: Vec<(&str, usize)> = self
            .nodes
            .keys()
            .map(|name| {
                let count = self.callees.get(name).map_or(0, |s| s.len());
                (name.as_str(), count)
            })
            .collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        ranked.truncate(n);
        ranked
    }
}

fn contains_word(text: &str, word: &str) -> bool {
    let bytes = text.as_bytes();
    let wbytes = word.as_bytes();
    let wlen = wbytes.len();
    if wlen == 0 {
        return false;
    }
    let mut i = 0;
    while i + wlen <= bytes.len() {
        if &bytes[i..i + wlen] == wbytes {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_ok = i + wlen >= bytes.len() || !is_ident_byte(bytes[i + wlen]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
