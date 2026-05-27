use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileAnnotation {
    pub subsystem: String,
    pub summary: String,
    pub notes: Option<String>,
    /// BLAKE3 of sorted function sources at annotation time (first 16 hex chars)
    pub file_hash: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SymbolAnnotation {
    /// Free-form semantic insight
    pub insight: String,
    // source_hash is now the outer map key — see AnnotationStore::symbols
}
