use anyhow::Result;
use sha2::{Digest, Sha256};
use tree_sitter::{Node, Tree};

use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};

pub struct EntityExtractor {
    file_path: String,
    repo: String,
    language: String,
}

impl EntityExtractor {
    pub fn new(file_path: String, repo: String, language: String) -> Self {
        Self {
            file_path,
            repo,
            language,
        }
    }

    pub fn extract(&self, tree: &Tree, source: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        // Create file entity
        let file_hash = hash_content(source);
        let file_entity = CodeEntity {
            kind: EntityKind::File,
            name: self.file_path.clone(),
            qualified_name: format!("{}:{}", self.repo, self.file_path),
            file_path: self.file_path.clone(),
            repo: self.repo.clone(),
            start_line: 0,
            end_line: source.lines().count() as u32,
            start_col: 0,
            end_col: 0,
            signature: None,
            body: None,
            body_hash: Some(file_hash),
            language: self.language.clone(),
        };
        entities.push(file_entity);

        // Walk the AST
        let root = tree.root_node();
        self.walk_node(root, source, &mut entities, &mut relations, None)?;

        Ok((entities, relations))
    }

    fn walk_node(
        &self,
        node: Node,
        source: &str,
        entities: &mut Vec<CodeEntity>,
        relations: &mut Vec<CodeRelation>,
        parent_qualified_name: Option<&str>,
    ) -> Result<()> {
        self.walk_node_depth(node, source, entities, relations, parent_qualified_name, 0)
    }

    fn walk_node_depth(
        &self,
        node: Node,
        source: &str,
        entities: &mut Vec<CodeEntity>,
        relations: &mut Vec<CodeRelation>,
        parent_qualified_name: Option<&str>,
        depth: usize,
    ) -> Result<()> {
        if depth > 100 { return Ok(()); }

        let kind_str = node.kind();

        match kind_str {
            // Functions / Methods
            "function_declaration"
            | "function_definition"
            | "method_definition"
            | "method_declaration"
            | "function_item"
            | "func_literal" => {
                if let Some(entity) = self.extract_function(node, source, parent_qualified_name)? {
                    let qname = entity.qualified_name.clone();

                    // file -> contains -> function
                    let file_qname = format!("{}:{}", self.repo, self.file_path);
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: parent_qualified_name.unwrap_or(&file_qname).to_string(),
                        to_entity: qname.clone(),
                        from_table: if parent_qualified_name.is_some() { "class".to_string() } else { "file".to_string() },
                        to_table: "function".to_string(),
                        metadata: None,
                    });

                    // Extract call sites within the function body
                    self.extract_calls(node, source, &qname, relations);

                    entities.push(entity);

                    // Continue walking children for nested items
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            self.walk_node_depth(child, source, entities, relations, Some(&qname), depth + 1)?;
                        }
                    }
                    return Ok(());
                }
            }

            // Classes / Structs / Interfaces
            "class_declaration"
            | "class_definition"
            | "struct_item"
            | "interface_declaration"
            | "trait_item"
            | "enum_item"
            | "type_declaration" => {
                if let Some(entity) = self.extract_class(node, source)? {
                    let qname = entity.qualified_name.clone();

                    let file_qname = format!("{}:{}", self.repo, self.file_path);
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: file_qname,
                        to_entity: qname.clone(),
                        from_table: "file".to_string(),
                        to_table: "class".to_string(),
                        metadata: None,
                    });

                    // Extract inheritance
                    self.extract_inheritance(node, source, &qname, relations);

                    entities.push(entity);

                    // Walk children with class as parent
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            self.walk_node_depth(child, source, entities, relations, Some(&qname), depth + 1)?;
                        }
                    }
                    return Ok(());
                }
            }

            // Imports
            "import_statement"
            | "import_declaration"
            | "use_declaration"
            | "import_from_statement" => {
                if let Some(entity) = self.extract_import(node, source)? {
                    let file_qname = format!("{}:{}", self.repo, self.file_path);
                    relations.push(CodeRelation {
                        kind: RelationKind::Imports,
                        from_entity: file_qname,
                        to_entity: entity.qualified_name.clone(),
                        from_table: "file".to_string(),
                        to_table: "import_decl".to_string(),
                        metadata: None,
                    });
                    entities.push(entity);
                }
            }

            _ => {}
        }

        // Recurse into children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                self.walk_node_depth(child, source, entities, relations, parent_qualified_name, depth + 1)?;
            }
        }

        Ok(())
    }

    fn extract_function(
        &self,
        node: Node,
        source: &str,
        parent: Option<&str>,
    ) -> Result<Option<CodeEntity>> {
        let name = self.find_child_text(node, "name", source)
            .or_else(|| self.find_child_text(node, "identifier", source));

        let name = match name {
            Some(n) => n,
            None => return Ok(None),
        };

        let qualified_name = match parent {
            Some(p) => format!("{}::{}", p, name),
            None => format!("{}:{}:{}", self.repo, self.file_path, name),
        };

        let body_text = node.utf8_text(source.as_bytes()).unwrap_or("");
        let body_hash = hash_content(body_text);

        // Build signature from parameters
        let signature = self.build_function_signature(node, source, &name);

        let kind = if parent.is_some() {
            EntityKind::Method
        } else {
            EntityKind::Function
        };

        Ok(Some(CodeEntity {
            kind,
            name,
            qualified_name,
            file_path: self.file_path.clone(),
            repo: self.repo.clone(),
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            start_col: node.start_position().column as u32,
            end_col: node.end_position().column as u32,
            signature: Some(signature),
            body: Some(body_text.to_string()),
            body_hash: Some(body_hash),
            language: self.language.clone(),
        }))
    }

    fn extract_class(&self, node: Node, source: &str) -> Result<Option<CodeEntity>> {
        let name = self.find_child_text(node, "name", source)
            .or_else(|| self.find_child_text(node, "identifier", source));

        let name = match name {
            Some(n) => n,
            None => return Ok(None),
        };

        let qualified_name = format!("{}:{}:{}", self.repo, self.file_path, name);

        let kind = match node.kind() {
            "struct_item" => EntityKind::Struct,
            "interface_declaration" => EntityKind::Interface,
            "trait_item" => EntityKind::Trait,
            "enum_item" => EntityKind::Enum,
            _ => EntityKind::Class,
        };

        Ok(Some(CodeEntity {
            kind,
            name,
            qualified_name,
            file_path: self.file_path.clone(),
            repo: self.repo.clone(),
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            start_col: node.start_position().column as u32,
            end_col: node.end_position().column as u32,
            signature: None,
            body: None,
            body_hash: None,
            language: self.language.clone(),
        }))
    }

    fn extract_import(&self, node: Node, source: &str) -> Result<Option<CodeEntity>> {
        let text = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
        if text.is_empty() {
            return Ok(None);
        }

        let name = text.lines().next().unwrap_or(&text).trim().to_string();
        let qualified_name = format!("{}:{}:import:{}", self.repo, self.file_path, name);

        Ok(Some(CodeEntity {
            kind: EntityKind::Import,
            name,
            qualified_name,
            file_path: self.file_path.clone(),
            repo: self.repo.clone(),
            start_line: node.start_position().row as u32 + 1,
            end_line: node.end_position().row as u32 + 1,
            start_col: node.start_position().column as u32,
            end_col: node.end_position().column as u32,
            signature: None,
            body: Some(text),
            body_hash: None,
            language: self.language.clone(),
        }))
    }

    fn extract_calls(
        &self,
        node: Node,
        source: &str,
        caller_qname: &str,
        relations: &mut Vec<CodeRelation>,
    ) {
        let mut cursor = node.walk();
        self.walk_for_calls(&mut cursor, source, caller_qname, relations);
    }

    fn walk_for_calls(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        caller_qname: &str,
        relations: &mut Vec<CodeRelation>,
    ) {
        let node = cursor.node();

        // Cover call node types across all supported languages:
        // Rust/TS/JS: call_expression   Python: call   Dart: invocation_expression
        // Java: method_invocation   Go: call_expression   Elixir: call
        let kind = node.kind();
        if kind == "call_expression"
            || kind == "method_invocation"
            || kind == "call"
            || kind == "invocation_expression"
        {
            if let Some(callee) = self.extract_callee_name(node, source) {
                relations.push(CodeRelation {
                    kind: RelationKind::Calls,
                    from_entity: caller_qname.to_string(),
                    to_entity: callee,
                    from_table: "function".to_string(),
                    to_table: "function".to_string(),
                    metadata: Some(serde_json::json!({
                        "line": node.start_position().row + 1,
                    })),
                });
            }
        }

        if cursor.goto_first_child() {
            loop {
                self.walk_for_calls(cursor, source, caller_qname, relations);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    fn extract_callee_name(&self, node: Node, source: &str) -> Option<String> {
        // Try standard field names across languages
        for field in &["function", "name", "method", "selector"] {
            if let Some(child) = node.child_by_field_name(field) {
                let text = child.utf8_text(source.as_bytes()).ok()?.trim().to_string();
                if !text.is_empty() {
                    // For dotted access like obj.method(), extract just the method name
                    return Some(text.rsplit('.').next().unwrap_or(&text).to_string());
                }
            }
        }
        // Fallback: first named child or first child
        node.named_child(0)
            .or_else(|| node.child(0))
            .and_then(|c| c.utf8_text(source.as_bytes()).ok())
            .map(|s| {
                let t = s.trim();
                t.rsplit('.').next().unwrap_or(t).to_string()
            })
    }

    fn extract_inheritance(
        &self,
        node: Node,
        source: &str,
        class_qname: &str,
        relations: &mut Vec<CodeRelation>,
    ) {
        // Look for superclass/heritage clauses
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                let kind = child.kind();
                if kind == "class_heritage"
                    || kind == "superclass"
                    || kind == "extends_clause"
                    || kind == "implements_clause"
                    || kind == "super_interfaces"
                {
                    let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                    let rel_kind = if kind.contains("implements") {
                        RelationKind::Implements
                    } else {
                        RelationKind::Inherits
                    };
                    relations.push(CodeRelation {
                        kind: rel_kind,
                        from_entity: class_qname.to_string(),
                        to_entity: text.trim().to_string(),
                        from_table: "class".to_string(),
                        to_table: "class".to_string(),
                        metadata: None,
                    });
                }
            }
        }
    }

    fn find_child_text(&self, node: Node, field: &str, source: &str) -> Option<String> {
        node.child_by_field_name(field)
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .map(|s| s.trim().to_string())
    }

    fn build_function_signature(&self, node: Node, source: &str, name: &str) -> String {
        // Try to extract just the signature line (before the body)
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        let first_line = text.lines().next().unwrap_or(name);
        first_line.to_string()
    }
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
