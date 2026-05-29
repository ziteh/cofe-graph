pub const QUICK_REFERENCE: &str = include_str!("quick_reference.md");
pub const RULES_OF_THUMB: &str = include_str!("rules_of_thumb.md");
pub const WORKFLOWS: &str = include_str!("workflows.md");

pub const RESOURCES: &[(&str, &str, &str, &str)] = &[
    (
        "graph://quick-reference",
        "Quick Reference",
        "Question-to-tool lookup table for code analysis",
        QUICK_REFERENCE,
    ),
    (
        "graph://rules-of-thumb",
        "Rules of Thumb",
        "Best practices for using code analysis tools effectively",
        RULES_OF_THUMB,
    ),
    (
        "graph://workflows",
        "Common Workflows",
        "Step-by-step guides for common code analysis tasks",
        WORKFLOWS,
    ),
];
