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
    /// Raw source of the full definition, trimmed to 500 chars
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
    /// Preprocessor conditions wrapping this node
    #[serde(default)]
    pub conditions: Vec<String>,
    /// The value/body text, trimmed to 200 chars
    pub value: Option<String>,
    pub file: PathBuf,
    pub line: u32,
}
