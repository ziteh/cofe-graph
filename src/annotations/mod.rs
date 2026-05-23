mod types;
pub use types::*;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cache::COFE_DATA_DIR;

pub struct AnnotationStore {
    pub files: HashMap<String, FileAnnotation>,
    path: Option<PathBuf>,
}

impl AnnotationStore {
    pub fn load(base: &Path) -> Self {
        let path = base.join(COFE_DATA_DIR).join("annotations.json");
        let files = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            files,
            path: Some(path),
        }
    }

    pub fn save(&self) {
        if let Some(ref p) = self.path {
            if let Ok(s) = serde_json::to_string_pretty(&self.files) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}
