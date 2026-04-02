use anyhow::Result;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use super::ContentParser;

/// Parses package manifest files (package.json, Cargo.toml)
pub struct PackageParser;

impl ContentParser for PackageParser {
    fn name(&self) -> &str { "package" }
    fn extensions(&self) -> &[&str] { &[] } // Detected by filename

    fn parse(&self, file_path: &str, source: &str, repo: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let filename = file_path.rsplit('/').next().unwrap_or(file_path);
        match filename {
            "package.json" => parse_package_json(file_path, source, repo),
            "Cargo.toml" => parse_cargo_toml(file_path, source, repo),
            _ => Ok((Vec::new(), Vec::new())),
        }
    }
}

fn parse_package_json(file_path: &str, source: &str, repo: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
    let mut entities = Vec::new();
    let mut relations = Vec::new();

    let file_qname = format!("{}:{}", repo, file_path);
    entities.push(CodeEntity {
        kind: EntityKind::File, name: file_path.to_string(),
        qualified_name: file_qname.clone(), file_path: file_path.to_string(),
        repo: repo.to_string(), start_line: 0, end_line: source.lines().count() as u32,
        start_col: 0, end_col: 0, signature: None, body: None, body_hash: None,
        language: "json".to_string(),
    });

    let value: serde_json::Value = match serde_json::from_str(source) {
        Ok(v) => v,
        Err(_) => return Ok((entities, relations)),
    };

    // Package name (fallback to filename-derived name)
    let pkg_name = value.get("name").and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            file_path.rsplit(&['/', '\\']).nth(1).unwrap_or("unknown").to_string()
        });
    {
        let name = &pkg_name;
        let version = value.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        let qname = format!("{}:package:{}", file_qname, name);
        entities.push(CodeEntity {
            kind: EntityKind::Package, name: name.to_string(),
            qualified_name: qname.clone(), file_path: file_path.to_string(),
            repo: repo.to_string(), start_line: 0, end_line: 0, start_col: 0, end_col: 0,
            signature: Some(format!("{}@{}", name, version)),
            body: value.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()),
            body_hash: None, language: "json".to_string(),
        });
        relations.push(CodeRelation {
            kind: RelationKind::Contains, from_entity: file_qname.clone(),
            to_entity: qname.clone(),
            from_table: "file".to_string(),
            to_table: "package".to_string(),
            metadata: None,
        });

        // Dependencies
        for dep_key in &["dependencies", "devDependencies", "peerDependencies"] {
            if let Some(deps) = value.get(*dep_key).and_then(|d| d.as_object()) {
                for (dep_name, dep_version) in deps {
                    let dqname = format!("{}:dep:{}", qname, dep_name);
                    entities.push(CodeEntity {
                        kind: EntityKind::Dependency, name: dep_name.clone(),
                        qualified_name: dqname.clone(), file_path: file_path.to_string(),
                        repo: repo.to_string(), start_line: 0, end_line: 0, start_col: 0, end_col: 0,
                        signature: Some(format!("{}: {}", dep_key, dep_version)),
                        body: dep_version.as_str().map(|s| s.to_string()),
                        body_hash: None, language: "json".to_string(),
                    });
                    relations.push(CodeRelation {
                        kind: RelationKind::DependsOnPackage,
                        from_entity: qname.clone(), to_entity: dqname,
                        from_table: "package".to_string(),
                        to_table: "package".to_string(),
                        metadata: None,
                    });
                }
            }
        }

        // Scripts
        if let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) {
            for (script_name, script_cmd) in scripts {
                let sqname = format!("{}:script:{}", qname, script_name);
                entities.push(CodeEntity {
                    kind: EntityKind::Script, name: script_name.clone(),
                    qualified_name: sqname.clone(), file_path: file_path.to_string(),
                    repo: repo.to_string(), start_line: 0, end_line: 0, start_col: 0, end_col: 0,
                    signature: Some(format!("npm run {}", script_name)),
                    body: script_cmd.as_str().map(|s| s.to_string()),
                    body_hash: None, language: "json".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::RunsScript,
                    from_entity: qname.clone(), to_entity: sqname,
                    from_table: "package".to_string(),
                    to_table: "package".to_string(),
                    metadata: None,
                });
            }
        }
    }

    Ok((entities, relations))
}

fn parse_cargo_toml(file_path: &str, source: &str, repo: &str) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
    let mut entities = Vec::new();
    let mut relations = Vec::new();

    let file_qname = format!("{}:{}", repo, file_path);
    entities.push(CodeEntity {
        kind: EntityKind::File, name: file_path.to_string(),
        qualified_name: file_qname.clone(), file_path: file_path.to_string(),
        repo: repo.to_string(), start_line: 0, end_line: source.lines().count() as u32,
        start_col: 0, end_col: 0, signature: None, body: None, body_hash: None,
        language: "toml".to_string(),
    });

    let value: toml::Value = match toml::from_str(source) {
        Ok(v) => v,
        Err(_) => return Ok((entities, relations)),
    };

    // Package info
    if let Some(pkg) = value.get("package") {
        let name = pkg.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
        let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("0.0.0");
        let qname = format!("{}:package:{}", file_qname, name);

        entities.push(CodeEntity {
            kind: EntityKind::Package, name: name.to_string(),
            qualified_name: qname.clone(), file_path: file_path.to_string(),
            repo: repo.to_string(), start_line: 0, end_line: 0, start_col: 0, end_col: 0,
            signature: Some(format!("{}@{}", name, version)),
            body: pkg.get("description").and_then(|d| d.as_str()).map(|s| s.to_string()),
            body_hash: None, language: "toml".to_string(),
        });

        // Dependencies
        if let Some(deps) = value.get("dependencies").and_then(|d| d.as_table()) {
            for (dep_name, dep_spec) in deps {
                let dqname = format!("{}:dep:{}", qname, dep_name);
                let version_str = match dep_spec {
                    toml::Value::String(s) => s.clone(),
                    toml::Value::Table(t) => t.get("version").and_then(|v| v.as_str()).unwrap_or("*").to_string(),
                    _ => "*".to_string(),
                };
                entities.push(CodeEntity {
                    kind: EntityKind::Dependency, name: dep_name.clone(),
                    qualified_name: dqname.clone(), file_path: file_path.to_string(),
                    repo: repo.to_string(), start_line: 0, end_line: 0, start_col: 0, end_col: 0,
                    signature: Some(version_str), body: None, body_hash: None,
                    language: "toml".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::DependsOnPackage,
                    from_entity: qname.clone(), to_entity: dqname,
                    from_table: "package".to_string(),
                    to_table: "package".to_string(),
                    metadata: None,
                });
            }
        }
    }

    Ok((entities, relations))
}
