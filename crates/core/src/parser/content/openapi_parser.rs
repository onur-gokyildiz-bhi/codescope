use anyhow::Result;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use super::ContentParser;

/// Parses OpenAPI spec files (detects via content, not extension)
/// Works on YAML/JSON files that contain "openapi:" or "swagger:" keys
pub struct OpenApiParser;

impl ContentParser for OpenApiParser {
    fn name(&self) -> &str { "openapi" }
    fn extensions(&self) -> &[&str] { &[] } // Detected by content, not extension

    fn parse(&self, file_path: &str, source: &str, repo: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let file_qname = format!("{}:{}", repo, file_path);

        // Try to parse as JSON first, then YAML
        let value: serde_json::Value = if let Ok(v) = serde_json::from_str(source) {
            v
        } else if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(source) {
            // Convert YAML to JSON value
            serde_json::to_value(&yaml).unwrap_or(serde_json::Value::Null)
        } else {
            return Ok((entities, relations));
        };

        // Check if this is an OpenAPI spec
        let is_openapi = value.get("openapi").is_some() || value.get("swagger").is_some();
        if !is_openapi {
            return Ok((entities, relations));
        }

        entities.push(CodeEntity {
            kind: EntityKind::File, name: file_path.to_string(),
            qualified_name: file_qname.clone(), file_path: file_path.to_string(),
            repo: repo.to_string(), start_line: 0, end_line: source.lines().count() as u32,
            start_col: 0, end_col: 0, signature: None, body: None, body_hash: None,
            language: "openapi".to_string(),
        });

        // Extract paths (endpoints)
        if let Some(paths) = value.get("paths").and_then(|p| p.as_object()) {
            for (path, methods) in paths {
                if let Some(methods_obj) = methods.as_object() {
                    for (method, _op) in methods_obj {
                        let upper_method = method.to_uppercase();
                        if !["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS", "HEAD"].contains(&upper_method.as_str()) {
                            continue;
                        }
                        let name = format!("{} {}", upper_method, path);
                        let qname = format!("{}:endpoint:{}:{}", file_qname, method, path.replace('/', "_"));

                        entities.push(CodeEntity {
                            kind: EntityKind::ApiEndpoint,
                            name, qualified_name: qname.clone(),
                            file_path: file_path.to_string(), repo: repo.to_string(),
                            start_line: 0, end_line: 0, start_col: 0, end_col: 0,
                            signature: Some(format!("{} {}", upper_method, path)),
                            body: None, body_hash: None, language: "openapi".to_string(),
                        });
                        relations.push(CodeRelation {
                            kind: RelationKind::DefinesEndpoint,
                            from_entity: file_qname.clone(),
                            to_entity: qname, metadata: None,
                        });
                    }
                }
            }
        }

        // Extract schemas
        let schemas = value.pointer("/components/schemas")
            .or_else(|| value.pointer("/definitions"));
        if let Some(schemas_obj) = schemas.and_then(|s| s.as_object()) {
            for (name, schema) in schemas_obj {
                let qname = format!("{}:schema:{}", file_qname, name);
                entities.push(CodeEntity {
                    kind: EntityKind::ApiSchema,
                    name: name.clone(), qualified_name: qname.clone(),
                    file_path: file_path.to_string(), repo: repo.to_string(),
                    start_line: 0, end_line: 0, start_col: 0, end_col: 0,
                    signature: schema.get("type").and_then(|t| t.as_str()).map(|s| s.to_string()),
                    body: None, body_hash: None, language: "openapi".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains, from_entity: file_qname.clone(),
                    to_entity: qname.clone(), metadata: None,
                });

                // Extract fields
                if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
                    for (field_name, field_def) in props {
                        let fqname = format!("{}:field:{}", qname, field_name);
                        let field_type = field_def.get("type").and_then(|t| t.as_str()).unwrap_or("any");
                        entities.push(CodeEntity {
                            kind: EntityKind::ApiField,
                            name: field_name.clone(), qualified_name: fqname.clone(),
                            file_path: file_path.to_string(), repo: repo.to_string(),
                            start_line: 0, end_line: 0, start_col: 0, end_col: 0,
                            signature: Some(field_type.to_string()),
                            body: None, body_hash: None, language: "openapi".to_string(),
                        });
                        relations.push(CodeRelation {
                            kind: RelationKind::HasField,
                            from_entity: qname.clone(), to_entity: fqname, metadata: None,
                        });
                    }
                }
            }
        }

        Ok((entities, relations))
    }
}
