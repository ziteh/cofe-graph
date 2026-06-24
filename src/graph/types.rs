use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CallEdge {
    pub name: String,
    pub line: u32,
    pub caller_file: PathBuf,
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
    /// Preprocessor conditions wrapping this node (e.g. ["#ifdef NRF52840", "#else"])
    #[serde(default)]
    pub conditions: Vec<String>,
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
    pub definition: String,
    /// Preprocessor conditions wrapping this node
    #[serde(default)]
    pub conditions: Vec<String>,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GlobalVar {
    pub name: String,
    /// Raw declaration text (trimmed)
    pub decl: String,
    /// Preprocessor conditions wrapping this node
    #[serde(default)]
    pub conditions: Vec<String>,
    pub is_static: bool,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SymbolNode {
    pub name: String,
    pub kind: SymbolKind,
    /// Preprocessor conditions wrapping this node
    #[serde(default)]
    pub conditions: Vec<String>,
    pub value: Option<String>,
    pub file: PathBuf,
    pub line: u32,
}

#[derive(Default, Serialize, Deserialize)]
pub struct FileGraph {
    pub nodes: HashMap<String, Vec<FunctionNode>>,
    /// Forward edges: caller → callees with call site line numbers
    pub callees: HashMap<String, Vec<CallEdge>>,
    pub symbols: HashMap<String, Vec<SymbolNode>>,
    pub types: HashMap<String, Vec<TypeNode>>,
    pub globals: HashMap<String, Vec<GlobalVar>>,
}
