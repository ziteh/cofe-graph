pub mod analysis;
pub mod functions;
pub mod globals;
pub mod includes;
pub mod index;
pub mod search;
pub mod symbols;
pub mod traverse;
pub mod types;

/// Return the path relative to `root` (e.g. "main/infrared/infrared_app.c").
/// Falls back to the basename, then the full path string, if stripping fails.
pub(crate) fn rel_file(root: &std::path::Path, path: &std::path::Path) -> String {
    if let Ok(rel) = path.strip_prefix(root) {
        return rel.to_string_lossy().into_owned();
    }
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}
