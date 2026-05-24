pub mod analysis;
pub mod annotate;
pub mod functions;
pub mod globals;
pub mod includes;
pub mod index;
pub mod symbols;
pub mod traverse;
pub mod types;

/// Return just the filename component of a path (e.g. "usbd_conf.c").
/// Falls back to the full path string if the filename can't be extracted.
pub(crate) fn short_file(path: &std::path::Path) -> std::borrow::Cow<'_, str> {
    path.file_name()
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| path.to_string_lossy())
}
