// Per-file cache keyed by blake3 content hash.
//
// The key is always blake3(file bytes), computed from the current disk content.
// This means the cache is always accurate regardless of git state: clean tree,
// uncommitted edits, untracked files, or no git at all all behave identically.
//
// LRU eviction is mtime-based. The caller controls the max entry count so that
// it can be derived from the actual file count discovered during indexing.

use std::path::{Path, PathBuf};

use crate::graph::FileGraph;

pub const DATA_DIR_NAME: &str = ".cofe-graph";

pub struct Cache {
    pub dir: PathBuf,
    blobs_dir: PathBuf,
}

impl Cache {
    /// Open (or create) the cache.
    /// Returns `None` only if the directory cannot be created.
    pub fn open(base: &Path) -> Option<Self> {
        let dir = base.join(DATA_DIR_NAME);
        let blobs_dir = dir.join("cache").join("blobs");
        std::fs::create_dir_all(&blobs_dir).ok()?;

        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            let _ = std::fs::write(&gitignore, "# Auto-created\n*\n");
        }

        Some(Self { dir, blobs_dir })
    }

    /// Load the file graph for a single file by its blake3 content hash.
    /// Returns `None` on cache miss. Touches the file's mtime on hit.
    pub fn load_file_graph(&self, key: &str) -> Option<FileGraph> {
        let path = self.blob_path(key);
        let bytes = std::fs::read(&path).ok()?;
        let fg = bincode::deserialize(&bytes).ok()?;
        touch(&path);
        Some(fg)
    }

    /// Persist a file graph under `key`.
    pub fn save_file_graph(&self, key: &str, fg: &FileGraph) {
        let path = self.blob_path(key);
        if let Ok(bytes) = bincode::serialize(fg) {
            let _ = std::fs::write(path, bytes);
        }
    }

    /// Evict oldest entries until at most `max_entries` remain.
    pub fn evict_if_needed(&self, max_entries: usize) {
        evict_dir(&self.blobs_dir, "fgraph", max_entries);
    }

    fn blob_path(&self, key: &str) -> PathBuf {
        self.blobs_dir.join(format!("{key}.fgraph"))
    }
}

fn evict_dir(dir: &Path, ext: &str, max_entries: usize) {
    if max_entries == 0 {
        return;
    }
    let mut entries: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    collect_cache_entries(dir, ext, &mut entries);
    if entries.len() <= max_entries {
        return;
    }
    entries.sort_by_key(|(_, t)| *t);
    let to_remove = entries.len() - max_entries;
    for (path, _) in entries.iter().take(to_remove) {
        let _ = std::fs::remove_file(path);
    }
}

fn collect_cache_entries(dir: &Path, ext: &str, out: &mut Vec<(PathBuf, std::time::SystemTime)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.extension().and_then(|x| x.to_str()) == Some(ext)
            && let Ok(mtime) = std::fs::metadata(&p).and_then(|m| m.modified())
        {
            out.push((p, mtime));
        }
    }
}

fn touch(path: &Path) {
    let _ = filetime::set_file_mtime(path, filetime::FileTime::now());
}
