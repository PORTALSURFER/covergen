//! Markdown help-catalog parser for in-app contextual help.
//!
//! The catalog format is human-readable Markdown with two structured heading
//! levels consumed by the parser:
//! - `## Global`
//! - `## Node \`stable_id\``
//! - `### Param \`param_key\`` inside a node section
//!
//! Any non-empty text line inside a section becomes one in-app help line.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Parsed help documentation for one node entry.
#[derive(Clone, Debug, Default)]
pub(crate) struct NodeHelpDoc {
    /// Node-level prose lines.
    pub(crate) lines: Vec<String>,
    /// Parameter help keyed by stable parameter key.
    pub(crate) params: HashMap<String, Vec<String>>,
}

/// Parsed help catalog consumed by the GUI help modal.
#[derive(Clone, Debug, Default)]
pub(crate) struct HelpCatalog {
    /// Global top-level help lines.
    pub(crate) global_lines: Vec<String>,
    /// Node docs keyed by stable node id.
    pub(crate) nodes: HashMap<String, NodeHelpDoc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Section {
    None,
    Global,
    Node(String),
    Param { node_id: String, key: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Header {
    Global,
    Node(String),
    Param(String),
}

static HELP_CATALOG: OnceLock<HelpCatalog> = OnceLock::new();

/// Return parsed in-app help catalog from repository Markdown docs.
pub(crate) fn help_catalog() -> &'static HelpCatalog {
    HELP_CATALOG.get_or_init(|| parse_help_markdown(catalog_markdown()))
}

fn catalog_markdown() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/docs/help/in_app_help.md"
    ))
}

fn parse_help_markdown(markdown: &str) -> HelpCatalog {
    let mut catalog = HelpCatalog::default();
    let mut section = Section::None;
    let mut current_node: Option<String> = None;
    for raw_line in markdown.lines() {
        let line = raw_line.trim();
        if let Some(header) = parse_section_header(line) {
            let next = match header {
                Header::Global => {
                    current_node = None;
                    Section::Global
                }
                Header::Node(node_id) => {
                    current_node = Some(node_id.clone());
                    Section::Node(node_id)
                }
                Header::Param(key) => {
                    let Some(node_id) = current_node.clone() else {
                        section = Section::None;
                        continue;
                    };
                    Section::Param { node_id, key }
                }
            };
            section = next;
            ensure_section_exists(&mut catalog, &section);
            continue;
        }
        let Some(content) = parse_content_line(line) else {
            continue;
        };
        push_section_line(&mut catalog, &section, content);
    }
    catalog
}

fn parse_section_header(line: &str) -> Option<Header> {
    if line == "## Global" {
        return Some(Header::Global);
    }
    if let Some(node_id) = parse_heading_code(line, "## Node ") {
        return Some(Header::Node(node_id.to_string()));
    }
    if let Some(param_key) = parse_heading_code(line, "### Param ") {
        return Some(Header::Param(param_key.to_string()));
    }
    None
}

fn parse_heading_code<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(prefix)?;
    let code = rest.strip_prefix('`')?.strip_suffix('`')?;
    if code.is_empty() {
        return None;
    }
    Some(code)
}

fn parse_content_line(line: &str) -> Option<&str> {
    if line.is_empty() {
        return None;
    }
    if line.starts_with('#') {
        return None;
    }
    if let Some(item) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return Some(item.trim());
    }
    Some(line)
}

fn ensure_section_exists(catalog: &mut HelpCatalog, section: &Section) {
    match section {
        Section::None => {}
        Section::Global => {}
        Section::Node(node_id) => {
            catalog.nodes.entry(node_id.clone()).or_default();
        }
        Section::Param { .. } => {}
    }
}

fn push_section_line(catalog: &mut HelpCatalog, section: &Section, content: &str) {
    match section {
        Section::None => {}
        Section::Global => catalog.global_lines.push(content.to_string()),
        Section::Node(node_id) => {
            if let Some(node) = catalog.nodes.get_mut(node_id) {
                node.lines.push(content.to_string());
            }
        }
        Section::Param { node_id, key } => {
            let node = catalog.nodes.entry(node_id.clone()).or_default();
            node.params
                .entry(key.clone())
                .or_default()
                .push(content.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{help_catalog, parse_help_markdown, Section};

    #[test]
    fn parser_extracts_global_node_and_param_sections() {
        let markdown = r#"
## Global
- hello world

## Node `tex.solid`
Solid node docs.
### Param `color_r`
- Red channel docs.
"#;
        let catalog = parse_help_markdown(markdown);
        assert_eq!(catalog.global_lines, vec!["hello world".to_string()]);
        let node = catalog.nodes.get("tex.solid").expect("node docs");
        assert_eq!(node.lines, vec!["Solid node docs.".to_string()]);
        assert_eq!(
            node.params
                .get("color_r")
                .expect("param docs")
                .first()
                .map(String::as_str),
            Some("Red channel docs.")
        );
    }

    #[test]
    fn section_param_variant_keeps_key() {
        let section = Section::Param {
            node_id: "tex.solid".to_string(),
            key: "color_r".to_string(),
        };
        assert!(matches!(
            section,
            Section::Param {
                node_id,
                key
            } if node_id == "tex.solid" && key == "color_r"
        ));
    }

    #[test]
    fn embedded_catalog_contains_feedback_node_and_param_docs() {
        let catalog = help_catalog();
        let node = catalog
            .nodes
            .get("tex.feedback")
            .expect("embedded feedback docs should exist");
        assert!(
            !node.lines.is_empty(),
            "feedback node should include node-level docs"
        );
        assert!(
            node.params.contains_key("accumulation_tex"),
            "feedback docs should include accumulation_tex param docs"
        );
        assert!(
            node.params.contains_key("frame_gap"),
            "feedback docs should include frame_gap param docs"
        );
        assert!(
            node.params.contains_key("reset"),
            "feedback docs should include reset param docs"
        );
    }
}
