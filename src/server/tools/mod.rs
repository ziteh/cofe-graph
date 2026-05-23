pub mod analysis;
pub mod functions;
pub mod globals;
pub mod includes;
pub mod index;
pub mod symbols;
pub mod traverse;
pub mod types;

use crate::graph::FunctionNode;

pub(super) fn fmt_fn(n: &FunctionNode) -> String {
    let vis = if n.is_static { " [static]" } else { "" };
    format!("{}{} @ {}:{}", n.name, vis, n.file.display(), n.line)
}
