use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileAnnotation {
    pub subsystem: String,
    pub summary: String,
    #[serde(default)]
    pub key_functions: Vec<String>,
    pub notes: Option<String>,
    /// BLAKE3 of sorted function sources at annotation time (first 16 hex chars)
    pub file_hash: String,
}
