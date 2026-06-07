use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

const ANNOT_DIR: &str = ".cofe-graph/annotations";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AnnotationStore {
    // keyed by blake3 content hash of the file
    files: HashMap<String, String>,
}

impl AnnotationStore {
    pub fn load(base: &Path) -> Self {
        let dir = base.join(ANNOT_DIR);
        let files = read_json(&dir.join("files.json")).unwrap_or_default();
        Self { files }
    }

    pub fn save(&self, base: &Path) -> Result<(), String> {
        let dir = base.join(ANNOT_DIR);
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create annotations dir: {e}"))?;
        write_json(&dir.join("files.json"), &self.files)?;
        Ok(())
    }

    pub fn upsert_file(&mut self, sha: &str, summary: String) {
        self.files.insert(sha.to_string(), summary);
    }

    pub fn get_file_annotation(&self, sha: &str) -> Option<&str> {
        self.files.get(sha).map(String::as_str)
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
