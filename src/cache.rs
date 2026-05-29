// Two-level cache.
//
// Level 1 (full-graph):
//   Keyed by git HEAD. Loaded instantly on repeated checkouts to the same commit.
//
// Level 2 (per-file):
//   Keyed by git blob SHA (or blake3 of file contents when git is unavailable).
//   On L1 miss, only files whose content changed since the last indexed commit
//   need re-parsing; unchanged files are loaded from L2 and merged in memory.
//
// LRU eviction is mtime-based.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::graph::{CallGraph, FileGraph};

pub const DATA_DIR_NAME: &str = ".cofe-graph";

pub struct Cache {
    pub dir: PathBuf,
    cache_dir: PathBuf,
    blobs_dir: PathBuf,
    pub commit_hash: String,
    /// Max number of full-graph cache to keep. 0 = disable L1 cache.
    max_l1_entries: usize,
    /// Max number of per-file cache to keep. 0 = disable L2 cache.
    max_l2_entries: usize,
}

impl Cache {
    /// Open (or create) the cache for `base`.
    /// Returns `None` when `base` is not inside a git repo.
    pub fn open(base: &Path, max_l1_entries: usize, max_l2_entries: usize) -> Option<Self> {
        let commit_hash = git_head(base)?;
        let dir = base.join(DATA_DIR_NAME);
        let cache_dir = dir.join("cache");
        let blobs_dir = cache_dir.join("blobs");
        std::fs::create_dir_all(&blobs_dir).ok()?;

        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            let _ = std::fs::write(&gitignore, "# Auto-create\n*\n");
        }

        Some(Self {
            dir,
            commit_hash,
            cache_dir,
            blobs_dir,
            max_l1_entries,
            max_l2_entries,
        })
    }

    /// Load the full graph for the current HEAD.
    /// Returns `None` if L1 is disabled or on cache miss.
    /// Touches the file's mtime on hit.
    pub fn load(&self) -> Option<CallGraph> {
        if self.max_l1_entries == 0 {
            return None;
        }
        let path = self.l1_path();
        let bytes = std::fs::read(&path).ok()?;
        let graph = bincode::deserialize(&bytes).ok()?;
        touch(&path);
        Some(graph)
    }

    /// Persist the full graph under the current HEAD key,
    /// then run mtime-based eviction. No-op if L1 is disabled.
    pub fn save(&self, graph: &CallGraph) {
        if self.max_l1_entries == 0 {
            return;
        }
        if let Ok(bytes) = bincode::serialize(graph) {
            let path = self.l1_path();
            let _ = std::fs::write(&path, bytes);
            touch(&path);
        }
        self.evict_if_needed();
    }

    /// Load the file graph for a single file identified by `blob_sha`.
    /// Returns `None` if L2 is disabled or on cache miss.
    /// Touches the file's mtime on hit.
    pub fn load_file_graph(&self, blob_sha: &str) -> Option<FileGraph> {
        if self.max_l2_entries == 0 {
            return None;
        }
        let path = self.blob_path(blob_sha);
        let bytes = std::fs::read(&path).ok()?;
        let fg = bincode::deserialize(&bytes).ok()?;
        touch(&path);
        Some(fg)
    }

    /// Persist a file graph under `blob_sha`. No-op if L2 is disabled.
    pub fn save_file_graph(&self, blob_sha: &str, fg: &FileGraph) {
        if self.max_l2_entries == 0 {
            return;
        }
        let path = self.blob_path(blob_sha);
        if let Ok(bytes) = bincode::serialize(fg) {
            let _ = std::fs::write(path, bytes);
        }
    }

    /// Return a map of `absolute_path → git_blob_sha`
    pub fn ls_files(repo: &Path) -> Option<HashMap<PathBuf, String>> {
        git_ls_files(repo)
    }

    fn l1_path(&self) -> PathBuf {
        self.cache_dir.join(format!("{}.cgraph", self.commit_hash))
    }

    fn blob_path(&self, sha: &str) -> PathBuf {
        self.blobs_dir.join(format!("{sha}.fgraph"))
    }

    fn evict_if_needed(&self) {
        evict_dir(&self.cache_dir, "cgraph", self.max_l1_entries);
        evict_dir(&self.blobs_dir, "fgraph", self.max_l2_entries);
    }
}

fn evict_dir(dir: &Path, ext: &str, max_entries: usize) {
    // max_entries == 0 means the level is disabled; nothing was written, nothing to evict.
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

pub fn git_head(repo: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["-C", repo.to_str()?, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
    } else {
        None
    }
}

fn git_ls_files(repo: &Path) -> Option<HashMap<PathBuf, String>> {
    let out = Command::new("git")
        .args([
            "-C",
            repo.to_str()?,
            "ls-files",
            "--format=%(objectname) %(path)",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8(out.stdout).ok()?;
    let mut map = HashMap::new();
    for line in stdout.lines() {
        if let Some((hash, rel_path)) = line.split_once(' ') {
            map.insert(repo.join(rel_path.trim()), hash.trim().to_string());
        }
    }
    Some(map)
}
