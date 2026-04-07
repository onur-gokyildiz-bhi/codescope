use super::ContentParser;
use crate::{CodeEntity, CodeRelation, EntityKind, RelationKind};
use anyhow::Result;

/// Parses Gradle build files (.gradle, .gradle.kts)
/// Extracts plugins, dependencies, android config, and build settings.
pub struct GradleParser;

impl ContentParser for GradleParser {
    fn name(&self) -> &str {
        "gradle"
    }
    fn extensions(&self) -> &[&str] {
        &["gradle", "gradle.kts"]
    }

    fn parse(
        &self,
        file_path: &str,
        source: &str,
        repo: &str,
    ) -> Result<(Vec<CodeEntity>, Vec<CodeRelation>)> {
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let file_qname = format!("{}:{}", repo, file_path);
        entities.push(CodeEntity {
            kind: EntityKind::File,
            name: file_path.to_string(),
            qualified_name: file_qname.clone(),
            file_path: file_path.to_string(),
            repo: repo.to_string(),
            start_line: 0,
            end_line: source.lines().count() as u32,
            start_col: 0,
            end_col: 0,
            signature: None,
            body: None,
            body_hash: None,
            language: "gradle".to_string(),
        });

        let lines: Vec<&str> = source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            let line_num = (i + 1) as u32;

            // Plugins: id("com.android.application") or id "org.jetbrains.kotlin.android"
            if let Some(plugin) = extract_plugin(trimmed) {
                let qname = format!("{}:plugin:{}", file_qname, plugin);
                entities.push(CodeEntity {
                    kind: EntityKind::ConfigKey,
                    name: format!("plugin:{}", plugin),
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(),
                    repo: repo.to_string(),
                    start_line: line_num,
                    end_line: line_num,
                    start_col: 0,
                    end_col: 0,
                    signature: Some(trimmed.to_string()),
                    body: Some(plugin.to_string()),
                    body_hash: None,
                    language: "gradle".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains,
                    from_entity: file_qname.clone(),
                    to_entity: qname,
                    from_table: "file".to_string(),
                    to_table: "config".to_string(),
                    metadata: None,
                });
            }

            // Dependencies: implementation("group:artifact:version") or implementation "..."
            if let Some((scope, dep)) = extract_dependency(trimmed) {
                let qname = format!("{}:dep:{}", file_qname, dep);
                entities.push(CodeEntity {
                    kind: EntityKind::Dependency,
                    name: dep.clone(),
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(),
                    repo: repo.to_string(),
                    start_line: line_num,
                    end_line: line_num,
                    start_col: 0,
                    end_col: 0,
                    signature: Some(format!("{} {}", scope, dep)),
                    body: Some(trimmed.to_string()),
                    body_hash: None,
                    language: "gradle".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::DependsOnPackage,
                    from_entity: file_qname.clone(),
                    to_entity: qname,
                    from_table: "file".to_string(),
                    to_table: "package".to_string(),
                    metadata: None,
                });
            }

            // Key config values: applicationId, namespace, minSdk, targetSdk, versionName, etc.
            if let Some((key, value)) = extract_config_value(trimmed) {
                let qname = format!("{}:{}", file_qname, key);
                entities.push(CodeEntity {
                    kind: EntityKind::ConfigKey,
                    name: key.clone(),
                    qualified_name: qname.clone(),
                    file_path: file_path.to_string(),
                    repo: repo.to_string(),
                    start_line: line_num,
                    end_line: line_num,
                    start_col: 0,
                    end_col: 0,
                    signature: Some(trimmed.to_string()),
                    body: Some(value),
                    body_hash: None,
                    language: "gradle".to_string(),
                });
                relations.push(CodeRelation {
                    kind: RelationKind::Contains,
                    from_entity: file_qname.clone(),
                    to_entity: qname,
                    from_table: "file".to_string(),
                    to_table: "config".to_string(),
                    metadata: None,
                });
            }
        }

        Ok((entities, relations))
    }
}

fn extract_plugin(line: &str) -> Option<String> {
    // id("com.android.application") or id "plugin.name" or id 'plugin.name'
    if !line.starts_with("id") {
        return None;
    }
    extract_quoted_value(line)
}

fn extract_dependency(line: &str) -> Option<(String, String)> {
    let scopes = [
        "implementation",
        "api",
        "compileOnly",
        "runtimeOnly",
        "testImplementation",
        "androidTestImplementation",
        "kapt",
        "ksp",
        "annotationProcessor",
        "coreLibraryDesugaring",
        "debugImplementation",
        "releaseImplementation",
    ];

    for scope in &scopes {
        if line.starts_with(scope) {
            if let Some(dep) = extract_quoted_value(line) {
                return Some((scope.to_string(), dep));
            }
        }
    }
    None
}

fn extract_config_value(line: &str) -> Option<(String, String)> {
    let keys = [
        "applicationId",
        "namespace",
        "minSdk",
        "targetSdk",
        "compileSdk",
        "versionCode",
        "versionName",
        "ndkVersion",
        "jvmTarget",
        "sourceCompatibility",
        "targetCompatibility",
        "buildToolsVersion",
    ];

    for key in &keys {
        if let Some(after_key) = line.strip_prefix(key) {
            let rest = after_key.trim();
            let value = if let Some(v) = rest.strip_prefix('=') {
                v.trim().trim_matches(|c| c == '"' || c == '\'').to_string()
            } else if let Some(v) = extract_quoted_value(rest) {
                v
            } else {
                rest.to_string()
            };
            if !value.is_empty() {
                return Some((key.to_string(), value));
            }
        }
    }
    None
}

fn extract_quoted_value(text: &str) -> Option<String> {
    // Extract value from ("value"), "value", or 'value'
    let s = text.trim();
    for (open, close) in [
        ("(\"", "\")"),
        ("('", "')"),
        ("(", ")"),
        ("\"", "\""),
        ("'", "'"),
    ] {
        if let Some(start) = s.find(open) {
            let after = &s[start + open.len()..];
            if let Some(end) = after.find(close) {
                let val = after[..end].trim().to_string();
                if !val.is_empty() {
                    return Some(val);
                }
            }
        }
    }
    None
}
