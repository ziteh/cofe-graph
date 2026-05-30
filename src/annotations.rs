use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

const ANNOT_DIR: &str = ".cofe-graph/annotations";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleAnnotation {
    pub name: String,
    pub summary: String,
    pub files: Vec<String>,
}

#[derive(Debug, Default, Clone)]
pub struct AnnotationStore {
    pub modules: HashMap<String, ModuleAnnotation>,
    /// key: git blob SHA → summary
    files: HashMap<String, String>,
    /// key: "{blob_sha}::{symbol_name}" → summary
    symbols: HashMap<String, String>,
}

impl AnnotationStore {
    pub fn load(base: &Path) -> Self {
        let dir = base.join(ANNOT_DIR);
        let modules = read_json(&dir.join("modules.json")).unwrap_or_default();
        let files = read_json(&dir.join("files.json")).unwrap_or_default();
        let symbols = read_json(&dir.join("symbols.json")).unwrap_or_default();
        Self {
            modules,
            files,
            symbols,
        }
    }

    pub fn save(&self, base: &Path) -> Result<(), String> {
        let dir = base.join(ANNOT_DIR);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create annotations dir: {e}"))?;
        write_json(&dir.join("modules.json"), &self.modules)?;
        write_json(&dir.join("files.json"), &self.files)?;
        write_json(&dir.join("symbols.json"), &self.symbols)?;
        Ok(())
    }

    pub fn upsert_module(&mut self, m: ModuleAnnotation) {
        self.modules.insert(m.name.clone(), m);
    }

    pub fn upsert_file(&mut self, sha: &str, summary: String) {
        self.files.insert(sha.to_string(), summary);
    }

    pub fn upsert_symbol(&mut self, sha: &str, name: &str, summary: String) {
        self.symbols.insert(format!("{sha}::{name}"), summary);
    }

    pub fn get_file_annotation(&self, sha: &str) -> Option<&str> {
        self.files.get(sha).map(String::as_str)
    }

    pub fn get_symbol_annotation(&self, sha: &str, name: &str) -> Option<&str> {
        self.symbols
            .get(&format!("{sha}::{name}"))
            .map(String::as_str)
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value).map_err(|e| format!("serialize error: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("write error: {e}"))?;
    Ok(())
}
