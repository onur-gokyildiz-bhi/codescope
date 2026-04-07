//! Entity linker — resolves code entity references from conversation text.
//! Matches function names, file paths, struct/class names against known entities.

/// A reference from conversation text to a code entity in the graph
#[derive(Debug, Clone)]
pub struct CodeReference {
    pub name: String,
    pub entity_table: String,
    pub qualified_name: String,
}

/// Links conversation text to known code entities using string matching.
pub struct EntityLinker {
    /// Known entity names (qualified_name values from the graph)
    known: Vec<(String, String, String)>, // (name, table, qualified_name)
}

impl EntityLinker {
    /// Build a linker from a list of known entity names.
    /// Format: "table:name:qualified_name" per entry.
    pub fn new(known_entities: &[String]) -> Self {
        let known = known_entities
            .iter()
            .filter_map(|entry| {
                let parts: Vec<&str> = entry.splitn(3, ':').collect();
                if parts.len() == 3 {
                    Some((
                        parts[1].to_string(),
                        parts[0].to_string(),
                        parts[2].to_string(),
                    ))
                } else {
                    None
                }
            })
            .collect();

        Self { known }
    }

    /// Build directly from name/table/qname tuples (used by MCP tools)
    pub fn from_tuples(tuples: Vec<(String, String, String)>) -> Self {
        Self { known: tuples }
    }

    /// Find all code entity references in a text block.
    /// Uses word-boundary matching to avoid false positives.
    pub fn find_references(&self, text: &str) -> Vec<CodeReference> {
        let mut refs = Vec::new();
        let lower = text.to_lowercase();

        for (name, table, qname) in &self.known {
            // Skip very short names (too many false positives)
            if name.len() < 4 {
                continue;
            }

            let name_lower = name.to_lowercase();

            // Check for backtick-quoted references first (high confidence)
            let backtick_pattern = format!("`{}`", name);
            if text.contains(&backtick_pattern) {
                refs.push(CodeReference {
                    name: name.clone(),
                    entity_table: table.clone(),
                    qualified_name: qname.clone(),
                });
                continue;
            }

            // Check for word-boundary match
            if let Some(pos) = lower.find(&name_lower) {
                // Verify word boundaries
                let before_ok = pos == 0 || !lower.as_bytes()[pos - 1].is_ascii_alphanumeric();
                let after_pos = pos + name_lower.len();
                let after_ok = after_pos >= lower.len()
                    || !lower.as_bytes()[after_pos].is_ascii_alphanumeric();

                if before_ok && after_ok {
                    refs.push(CodeReference {
                        name: name.clone(),
                        entity_table: table.clone(),
                        qualified_name: qname.clone(),
                    });
                }
            }
        }

        // Deduplicate by qualified_name
        refs.sort_by(|a, b| a.qualified_name.cmp(&b.qualified_name));
        refs.dedup_by(|a, b| a.qualified_name == b.qualified_name);
        refs
    }
}
