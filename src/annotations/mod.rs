mod types;
pub use types::*;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::cache::DATA_DIR_NAME;

#[derive(Deserialize, Default)]
struct StoredData {
    #[serde(default)]
    files: HashMap<String, FileAnnotation>,
    /// name → { source_hash → annotation }
    #[serde(default)]
    symbols: HashMap<String, HashMap<String, SymbolAnnotation>>,
}

#[derive(Default)]
pub struct AnnotationStore {
    pub files: HashMap<String, FileAnnotation>,
    /// name → { source_hash → annotation }
    pub symbols: HashMap<String, HashMap<String, SymbolAnnotation>>,
    path: Option<PathBuf>,
}

impl AnnotationStore {
    /// Look up an annotation for `name` whose source matches `source`.
    pub fn get_symbol<'a>(&'a self, name: &str, source: &str) -> Option<&'a SymbolAnnotation> {
        let hash = blake3::hash(source.as_bytes()).to_hex()[..16].to_string();
        self.symbols.get(name)?.get(&hash)
    }
}

impl AnnotationStore {
    pub fn load(base: &Path) -> Self {
        let path = base.join(DATA_DIR_NAME).join("annotations.json");
        let data: StoredData = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            files: data.files,
            symbols: data.symbols,
            path: Some(path),
        }
    }

    pub fn save(&self) {
        if let Some(ref p) = self.path {
            let data = serde_json::json!({
                "files": self.files,
                "symbols": self.symbols,
            });
            if let Ok(s) = serde_json::to_string_pretty(&data) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}
