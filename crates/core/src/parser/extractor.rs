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

    pub fn extract(
        &self,
        tree: &Tree,
        source: &str,
    ) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
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
        if depth > 100 {
            return Ok(());
        }

        let kind_str = node.kind();

        match kind_str {
            // Functions / Methods
            "function_declaration"
            | "function_definition"
            | "method_definition"
            | "method_declaration"
            | "function_item"
            | "func_literal"
            | "function_signature"
            | "method_signature"
            | "getter_signature"
            | "setter_signature"
            | "constructor_signature"
            | "constant_constructor_signature"
            | "factory_constructor_signature"
            | "operator_signature" => {
                if let Some(entity) = self.extract_function(node, source, parent_qualified_name)? {
                    let qname = entity.qualified_name.clone();

                    // file -> contains -> function
                    let file_qname = format!("{}:{}", self.repo, self.file_path);
                    relations.push(CodeRelation {
                        kind: RelationKind::Contains,
                        from_entity: parent_qualified_name.unwrap_or(&file_qname).to_string(),
                        to_entity: qname.clone(),
                        from_table: if parent_qualified_name.is_some() {
                            "class".to_string()
                        } else {
                            "file".to_string()
                        },
                        to_table: "function".to_string(),
                        metadata: None,
                    });

                    // Extract call sites within the function body
                    self.extract_calls(node, source, &qname, relations, entities);

                    entities.push(entity);

                    // Continue walking children for nested items
                    for i in 0..node.child_count() {
                        if let Some(child) = node.child(i) {
                            self.walk_node_depth(
                                child,
                                source,
                                entities,
                                relations,
                                Some(&qname),
                                depth + 1,
                            )?;
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
            | "type_declaration"
            | "enum_declaration"
            | "extension_declaration"
            | "mixin_declaration"
            | "type_alias" => {
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
                            self.walk_node_depth(
                                child,
                                source,
                                entities,
                                relations,
                                Some(&qname),
                                depth + 1,
                            )?;
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
                self.walk_node_depth(
                    child,
                    source,
                    entities,
                    relations,
                    parent_qualified_name,
                    depth + 1,
                )?;
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
        let name = self
            .find_child_text(node, "name", source)
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
        let name = self
            .find_child_text(node, "name", source)
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
        entities: &mut Vec<CodeEntity>,
    ) {
        let mut cursor = node.walk();
        self.walk_for_calls_with_entities(&mut cursor, source, caller_qname, relations, entities);
    }

    fn walk_for_calls_with_entities(
        &self,
        cursor: &mut tree_sitter::TreeCursor,
        source: &str,
        caller_qname: &str,
        relations: &mut Vec<CodeRelation>,
        entities: &mut Vec<CodeEntity>,
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
                // Build qualified name for callee: assume same file first,
                // cross-file resolution happens post-insert via resolve_call_targets
                let callee_qname = format!("{}:{}:{}", self.repo, self.file_path, callee);
                relations.push(CodeRelation {
                    kind: RelationKind::Calls,
                    from_entity: caller_qname.to_string(),
                    to_entity: callee_qname,
                    from_table: "function".to_string(),
                    to_table: "function".to_string(),
                    metadata: Some(serde_json::json!({
                        "line": node.start_position().row + 1,
                        "raw_callee": callee,
                    })),
                });
            }
        }

        // Detect HTTP client calls (reqwest, fetch, axios, requests, http)
        if kind == "call_expression" || kind == "method_invocation" || kind == "call" {
            if let Some((http_method, url_pattern, raw_text)) = self.extract_http_call(node, source)
            {
                let call_qname = format!(
                    "{}:{}:http:{}:{}:L{}",
                    self.repo,
                    self.file_path,
                    http_method,
                    sanitize_url(&url_pattern),
                    node.start_position().row + 1,
                );
                entities.push(CodeEntity {
                    kind: EntityKind::HttpClientCall,
                    name: format!("{} {}", http_method, url_pattern),
                    qualified_name: call_qname.clone(),
                    file_path: self.file_path.clone(),
                    repo: self.repo.clone(),
                    start_line: node.start_position().row as u32 + 1,
                    end_line: node.end_position().row as u32 + 1,
                    start_col: node.start_position().column as u32,
                    end_col: node.end_position().column as u32,
                    signature: Some(format!("{} {}", http_method, url_pattern)),
                    body: Some(raw_text),
                    body_hash: None,
                    language: self.language.clone(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::CallsEndpoint,
                    from_entity: caller_qname.to_string(),
                    to_entity: call_qname,
                    from_table: "function".to_string(),
                    to_table: "http_call".to_string(),
                    metadata: Some(serde_json::json!({
                        "method": http_method,
                        "url_pattern": url_pattern,
                        "line": node.start_position().row + 1,
                    })),
                });
            }
        }

        if cursor.goto_first_child() {
            loop {
                self.walk_for_calls_with_entities(
                    cursor,
                    source,
                    caller_qname,
                    relations,
                    entities,
                );
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
                    let name = text.rsplit('.').next().unwrap_or(&text).to_string();
                    return Self::clean_callee_name(&name);
                }
            }
        }
        // Fallback: first named child or first child
        node.named_child(0)
            .or_else(|| node.child(0))
            .and_then(|c| c.utf8_text(source.as_bytes()).ok())
            .and_then(|s| {
                let t = s.trim();
                let name = t.rsplit('.').next().unwrap_or(t).to_string();
                Self::clean_callee_name(&name)
            })
    }

    /// Clean a callee name: keep only valid identifier characters.
    fn clean_callee_name(name: &str) -> Option<String> {
        let cleaned: String = name
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if cleaned.is_empty()
            || cleaned
                .chars()
                .next()
                .map(|c| c.is_numeric())
                .unwrap_or(true)
        {
            None
        } else {
            Some(cleaned)
        }
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

    /// Detect HTTP client calls and extract method + URL pattern.
    /// Supports: reqwest (Rust), fetch (JS/TS), axios (JS/TS), requests (Python),
    /// http/net/http (Go), HttpClient (C#/Java).
    fn extract_http_call(&self, node: Node, source: &str) -> Option<(String, String, String)> {
        let text = node.utf8_text(source.as_bytes()).ok()?;

        // Get the callee text (function/method being called)
        let callee_text = self.get_full_callee_text(node, source)?;
        let callee_lower = callee_text.to_lowercase();

        // Known HTTP client patterns: "receiver.method" or "function"
        let http_methods = [
            "get",
            "post",
            "put",
            "delete",
            "patch",
            "head",
            "options",
            // C# HttpClient async methods
            "getasync",
            "postasync",
            "putasync",
            "deleteasync",
            "patchasync",
            "sendasync",
            "getstringasync",
            "getbytearrayasync",
            "getstreamasync",
            // RestSharp
            "executeasync",
            "executegettaskasync",
            "executeposttaskasync",
        ];
        let http_clients = [
            "reqwest",
            "client",
            "http_client",
            "httpclient",
            "_httpclient",
            "_client",
            "axios",
            "fetch",
            "requests",
            "http",
            "net",
            "ureq",
            "hyper",
            "surf",
            // C# / .NET
            "restclient",
            "flurlclient",
            "webclient",
        ];

        let method = if callee_lower == "fetch" {
            "GET".to_string()
        } else {
            let parts: Vec<&str> = callee_lower.rsplitn(2, ['.', ':']).collect();
            if parts.len() == 2 {
                let method_part = parts[0];
                let receiver_part = parts[1].rsplit(['.', ':']).next().unwrap_or(parts[1]);
                if http_methods.contains(&method_part)
                    && http_clients.iter().any(|c| receiver_part.contains(c))
                {
                    // Map C# async methods to HTTP methods
                    if method_part.starts_with("get")
                        || method_part == "getstringasync"
                        || method_part == "getbytearrayasync"
                        || method_part == "getstreamasync"
                    {
                        "GET".to_string()
                    } else if method_part.starts_with("post")
                        || method_part == "executeposttaskasync"
                    {
                        "POST".to_string()
                    } else if method_part.starts_with("put") {
                        "PUT".to_string()
                    } else if method_part.starts_with("delete") {
                        "DELETE".to_string()
                    } else if method_part.starts_with("patch") {
                        "PATCH".to_string()
                    } else if method_part == "sendasync"
                        || method_part == "executeasync"
                        || method_part == "executegettaskasync"
                    {
                        "UNKNOWN".to_string()
                    } else {
                        method_part.to_uppercase()
                    }
                } else {
                    return None;
                }
            } else {
                return None;
            }
        };

        // Extract URL from the first string argument
        let url = self.extract_first_string_arg(node, source)?;

        // Clean URL: strip protocol, extract path
        let path = extract_url_path(&url);

        if path.is_empty() || path == "/" {
            return None;
        }

        Some((method, path, text.to_string()))
    }

    /// Get the full callee text including receiver (e.g., "reqwest::get", "axios.post")
    fn get_full_callee_text(&self, node: Node, source: &str) -> Option<String> {
        for field in &["function", "name", "method", "selector"] {
            if let Some(child) = node.child_by_field_name(field) {
                let text = child.utf8_text(source.as_bytes()).ok()?.trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
        node.named_child(0)
            .and_then(|c| c.utf8_text(source.as_bytes()).ok())
            .map(|s| s.trim().to_string())
    }

    /// Extract the first string literal argument from a call expression.
    fn extract_first_string_arg(&self, node: Node, source: &str) -> Option<String> {
        // Look for arguments node
        let args_node = node.child_by_field_name("arguments").or_else(|| {
            // Fallback: find parenthesized child
            (0..node.child_count())
                .filter_map(|i| node.child(i))
                .find(|c| c.kind() == "arguments" || c.kind() == "argument_list")
        })?;

        // Find first string literal in arguments
        for i in 0..args_node.child_count() {
            if let Some(child) = args_node.child(i) {
                let kind = child.kind();
                if kind == "string"
                    || kind == "string_literal"
                    || kind == "interpreted_string_literal"
                    || kind == "template_string"
                    || kind == "raw_string_literal"
                {
                    let text = child.utf8_text(source.as_bytes()).ok()?.trim().to_string();
                    // Remove quotes
                    let unquoted = text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                    return Some(unquoted.to_string());
                }
            }
        }
        None
    }
}

/// Extract URL path from a full or partial URL.
/// "https://api.example.com/users/{id}" → "/users/{id}"
/// "/api/users" → "/api/users"
fn extract_url_path(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        // Full URL: extract path after host
        if let Some(idx) = url.find("://") {
            let after_proto = &url[idx + 3..];
            if let Some(path_start) = after_proto.find('/') {
                return after_proto[path_start..].to_string();
            }
        }
        "/".to_string()
    } else if url.starts_with('/') {
        url.to_string()
    } else {
        format!("/{}", url)
    }
}

/// Sanitize URL for use in SurrealDB record IDs
fn sanitize_url(url: &str) -> String {
    url.replace(['/', '{', '}', ':', '.', '?', '&', '=', '#', ' '], "_")
        .replace("__", "_")
        .trim_matches('_')
        .to_string()
}

fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
