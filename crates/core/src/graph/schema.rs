use crate::DbHandle;
use anyhow::Result;
use serde::Deserialize;
use surrealdb::types::SurrealValue;

/// Current schema version. Bump when adding a new migration in `migrations.rs`.
/// Version 0 = legacy DBs without meta:schema row.
pub const SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Deserialize, SurrealValue)]
struct SchemaMetaRow {
    version: u32,
}

/// Read the current schema_version recorded in the `meta` table.
/// Returns 0 if no `meta:schema` row exists (old / uninitialized DB).
pub async fn get_schema_version(db: &DbHandle) -> Result<u32> {
    // Returns Option<SchemaMetaRow>
    let row: Option<SchemaMetaRow> = db.select(("meta", "schema")).await?;
    Ok(row.map(|r| r.version).unwrap_or(0))
}

/// Persist the schema_version to the `meta:schema` row (UPSERT).
pub async fn set_schema_version(db: &DbHandle, version: u32) -> Result<()> {
    db.query("UPSERT meta:schema SET version = $v")
        .bind(("v", version))
        .await?;
    Ok(())
}

/// Initialize the SurrealDB schema for the knowledge graph
pub async fn init_schema(db: &DbHandle) -> Result<()> {
    db.query(
        "
        -- === CODE ENTITY TABLES ===

        DEFINE TABLE IF NOT EXISTS file SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS path ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS hash ON file TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS repo ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS line_count ON file TYPE option<int>;
        DEFINE INDEX IF NOT EXISTS file_path ON file FIELDS path;
        DEFINE INDEX IF NOT EXISTS file_repo ON file FIELDS repo;

        DEFINE TABLE IF NOT EXISTS `function` SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON `function` TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON `function` TYPE string;
        DEFINE FIELD IF NOT EXISTS signature ON `function` TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS body_hash ON `function` TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS file_path ON `function` TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON `function` TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON `function` TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON `function` TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON `function` TYPE int;
        DEFINE FIELD IF NOT EXISTS complexity ON `function` TYPE option<int>;
        DEFINE FIELD IF NOT EXISTS embedding ON `function` TYPE option<array>;
        DEFINE FIELD IF NOT EXISTS binary_embedding ON `function` TYPE option<array>;
        DEFINE FIELD IF NOT EXISTS cuda_qualifier ON `function` TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS fn_name ON `function` FIELDS name;
        DEFINE INDEX IF NOT EXISTS fn_qname ON `function` FIELDS qualified_name UNIQUE;
        DEFINE INDEX IF NOT EXISTS fn_file ON `function` FIELDS file_path;

        DEFINE TABLE IF NOT EXISTS class SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON class TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON class TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON class TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON class TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON class TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON class TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON class TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON class TYPE int;
        DEFINE INDEX IF NOT EXISTS class_name ON class FIELDS name;
        DEFINE INDEX IF NOT EXISTS class_qname ON class FIELDS qualified_name UNIQUE;

        DEFINE TABLE IF NOT EXISTS module SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON module TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON module TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON module TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON module TYPE string;
        DEFINE INDEX IF NOT EXISTS mod_qname ON module FIELDS qualified_name UNIQUE;

        DEFINE TABLE IF NOT EXISTS variable SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON variable TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON variable TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON variable TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON variable TYPE string;
        DEFINE INDEX IF NOT EXISTS var_qname ON variable FIELDS qualified_name UNIQUE;

        DEFINE TABLE IF NOT EXISTS import_decl SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS body ON import_decl TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS imp_qname ON import_decl FIELDS qualified_name UNIQUE;

        -- === CONFIG ENTITY TABLE (JSON, YAML, TOML) ===

        DEFINE TABLE IF NOT EXISTS config SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON config TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON config TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON config TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON config TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON config TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON config TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON config TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON config TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON config TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS cfg_name ON config FIELDS name;
        DEFINE INDEX IF NOT EXISTS cfg_qname ON config FIELDS qualified_name UNIQUE;

        -- === DOCUMENTATION TABLE (Markdown) ===

        DEFINE TABLE IF NOT EXISTS doc SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON doc TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON doc TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON doc TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON doc TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON doc TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON doc TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON doc TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON doc TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON doc TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS doc_qname ON doc FIELDS qualified_name UNIQUE;

        -- === API TABLE (OpenAPI, Protobuf) ===

        DEFINE TABLE IF NOT EXISTS api SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON api TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON api TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON api TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON api TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON api TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON api TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON api TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON api TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON api TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS api_qname ON api FIELDS qualified_name UNIQUE;

        -- === DATABASE ENTITY TABLE (SQL) ===

        DEFINE TABLE IF NOT EXISTS db_entity SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON db_entity TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON db_entity TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON db_entity TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON db_entity TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON db_entity TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON db_entity TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON db_entity TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON db_entity TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON db_entity TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS db_qname ON db_entity FIELDS qualified_name UNIQUE;

        -- === INFRASTRUCTURE TABLE (Terraform, Dockerfile, K8s) ===

        DEFINE TABLE IF NOT EXISTS infra SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON infra TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON infra TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON infra TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON infra TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON infra TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON infra TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON infra TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON infra TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON infra TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS infra_qname ON infra FIELDS qualified_name UNIQUE;

        -- === PACKAGE TABLE (package.json, Cargo.toml) ===

        DEFINE TABLE IF NOT EXISTS package SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON package TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON package TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON package TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON package TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON package TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON package TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON package TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON package TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON package TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS pkg_qname ON package FIELDS qualified_name UNIQUE;

        -- === SKILL/KNOWLEDGE GRAPH TABLE (arscontexta-style) ===

        DEFINE TABLE IF NOT EXISTS skill SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON skill TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON skill TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON skill TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON skill TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON skill TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON skill TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON skill TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON skill TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON skill TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS description ON skill TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS node_type ON skill TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS created ON skill TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS skill_name ON skill FIELDS name;
        DEFINE INDEX IF NOT EXISTS skill_qname ON skill FIELDS qualified_name UNIQUE;
        DEFINE INDEX IF NOT EXISTS skill_file_repo ON skill FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS skill_repo ON skill FIELDS repo;

        -- === HTTP CLIENT CALL TABLE (cross-service linking) ===

        DEFINE TABLE IF NOT EXISTS http_call SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON http_call TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON http_call TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON http_call TYPE string;
        DEFINE FIELD IF NOT EXISTS method ON http_call TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS url_pattern ON http_call TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS file_path ON http_call TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON http_call TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON http_call TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON http_call TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON http_call TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON http_call TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS http_call_qname ON http_call FIELDS qualified_name UNIQUE;
        DEFINE INDEX IF NOT EXISTS http_call_method ON http_call FIELDS method;
        DEFINE INDEX IF NOT EXISTS http_call_file_repo ON http_call FIELDS file_path, repo;

        -- === CONVERSATION TABLES (Claude session transcripts) ===

        DEFINE TABLE IF NOT EXISTS conversation SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON conversation TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON conversation TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON conversation TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS language ON conversation TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON conversation TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS hash ON conversation TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp ON conversation TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS conv_qname ON conversation FIELDS qualified_name UNIQUE;
        DEFINE INDEX IF NOT EXISTS conv_repo ON conversation FIELDS repo;

        DEFINE TABLE IF NOT EXISTS conv_topic SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON conv_topic TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON conv_topic TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON conv_topic TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON conv_topic TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON conv_topic TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON conv_topic TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON conv_topic TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS language ON conv_topic TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON conv_topic TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp ON conv_topic TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS topic_qname ON conv_topic FIELDS qualified_name UNIQUE;

        DEFINE FIELD IF NOT EXISTS scope ON conv_topic TYPE option<string>;

        DEFINE TABLE IF NOT EXISTS decision SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON decision TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON decision TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON decision TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON decision TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON decision TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON decision TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON decision TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS language ON decision TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON decision TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp ON decision TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS dec_qname ON decision FIELDS qualified_name UNIQUE;
        DEFINE INDEX IF NOT EXISTS dec_repo ON decision FIELDS repo;

        DEFINE FIELD IF NOT EXISTS tier ON decision TYPE option<int> DEFAULT 2;
        DEFINE FIELD IF NOT EXISTS rationale ON decision TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS scope ON decision TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS agent ON decision TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS agent ON problem TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS agent ON solution TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS agent ON conv_topic TYPE option<string>;

        DEFINE TABLE IF NOT EXISTS problem SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON problem TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON problem TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON problem TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON problem TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON problem TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON problem TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON problem TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS language ON problem TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON problem TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp ON problem TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS prob_qname ON problem FIELDS qualified_name UNIQUE;

        DEFINE FIELD IF NOT EXISTS tier ON problem TYPE option<int> DEFAULT 2;
        DEFINE FIELD IF NOT EXISTS scope ON problem TYPE option<string>;

        DEFINE TABLE IF NOT EXISTS solution SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON solution TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON solution TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON solution TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON solution TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON solution TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON solution TYPE int;
        DEFINE FIELD IF NOT EXISTS body ON solution TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS language ON solution TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON solution TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp ON solution TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS sol_qname ON solution FIELDS qualified_name UNIQUE;

        DEFINE FIELD IF NOT EXISTS tier ON solution TYPE option<int> DEFAULT 2;
        DEFINE FIELD IF NOT EXISTS scope ON solution TYPE option<string>;

        -- === EDGE TABLES (TYPE RELATION required for RELATE statements) ===

        DEFINE TABLE contains TYPE RELATION SCHEMAFULL;
        DEFINE TABLE calls TYPE RELATION SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS line ON calls TYPE option<int>;
        DEFINE FIELD IF NOT EXISTS raw_callee ON calls TYPE option<string>;
        DEFINE TABLE imports TYPE RELATION SCHEMAFULL;
        DEFINE TABLE inherits TYPE RELATION SCHEMAFULL;
        DEFINE TABLE implements TYPE RELATION SCHEMAFULL;
        DEFINE TABLE uses TYPE RELATION SCHEMAFULL;
        DEFINE TABLE modified_in TYPE RELATION SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS commit_hash ON modified_in TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp ON modified_in TYPE option<datetime>;
        DEFINE FIELD IF NOT EXISTS change_type ON modified_in TYPE option<string>;
        DEFINE TABLE depends_on TYPE RELATION SCHEMAFULL;
        DEFINE TABLE configures TYPE RELATION SCHEMAFULL;
        DEFINE TABLE defines_endpoint TYPE RELATION SCHEMAFULL;
        DEFINE TABLE has_field TYPE RELATION SCHEMAFULL;
        DEFINE TABLE references TYPE RELATION SCHEMAFULL;
        DEFINE TABLE depends_on_package TYPE RELATION SCHEMAFULL;
        DEFINE TABLE runs_script TYPE RELATION SCHEMAFULL;
        DEFINE TABLE discussed_in TYPE RELATION SCHEMAFULL;
        DEFINE TABLE decided_about TYPE RELATION SCHEMAFULL;
        DEFINE TABLE solves_for TYPE RELATION SCHEMAFULL;
        DEFINE TABLE co_discusses TYPE RELATION SCHEMAFULL;
        DEFINE TABLE links_to TYPE RELATION SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS context ON links_to TYPE option<string>;
        DEFINE TABLE calls_endpoint TYPE RELATION SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS method ON calls_endpoint TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS url_pattern ON calls_endpoint TYPE option<string>;

        -- === COMPOSITE INDEXES (performance: file_path+repo queries) ===

        DEFINE INDEX IF NOT EXISTS fn_file_repo ON `function` FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS fn_repo ON `function` FIELDS repo;
        DEFINE INDEX IF NOT EXISTS class_file_repo ON class FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS class_repo ON class FIELDS repo;
        DEFINE INDEX IF NOT EXISTS file_path_repo ON file FIELDS path, repo UNIQUE;
        DEFINE INDEX IF NOT EXISTS mod_file_repo ON module FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS var_file_repo ON variable FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS imp_file_repo ON import_decl FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS cfg_file_repo ON config FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS doc_file_repo ON doc FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS api_file_repo ON api FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS db_file_repo ON db_entity FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS infra_file_repo ON infra FIELDS file_path, repo;
        DEFINE INDEX IF NOT EXISTS pkg_file_repo ON package FIELDS file_path, repo;

        -- === ORDER BY / SORT INDEXES (hot-path timestamp + language filters) ===
        -- Added in SCHEMA_VERSION = 3. See graph/migrations.rs.

        DEFINE INDEX IF NOT EXISTS know_updated_at ON knowledge FIELDS updated_at;
        DEFINE INDEX IF NOT EXISTS decision_timestamp ON decision FIELDS timestamp;
        DEFINE INDEX IF NOT EXISTS problem_timestamp ON problem FIELDS timestamp;
        DEFINE INDEX IF NOT EXISTS solution_timestamp ON solution FIELDS timestamp;
        DEFINE INDEX IF NOT EXISTS conv_topic_timestamp ON conv_topic FIELDS timestamp;
        DEFINE INDEX IF NOT EXISTS conversation_timestamp ON conversation FIELDS timestamp;
        DEFINE INDEX IF NOT EXISTS file_language ON file FIELDS language;

        -- === FULL-TEXT SEARCH INDEXES (speeds up name search queries) ===

        DEFINE ANALYZER IF NOT EXISTS name_analyzer TOKENIZERS blank, class FILTERS lowercase;
        DEFINE INDEX IF NOT EXISTS fn_name_search ON `function` FIELDS name FULLTEXT ANALYZER name_analyzer BM25;
        DEFINE INDEX IF NOT EXISTS class_name_search ON class FIELDS name FULLTEXT ANALYZER name_analyzer BM25;
        DEFINE INDEX IF NOT EXISTS config_name_search ON config FIELDS name FULLTEXT ANALYZER name_analyzer BM25;
        DEFINE INDEX IF NOT EXISTS doc_name_search ON doc FIELDS name FULLTEXT ANALYZER name_analyzer BM25;
        DEFINE INDEX IF NOT EXISTS pkg_name_search ON package FIELDS name FULLTEXT ANALYZER name_analyzer BM25;
        DEFINE INDEX IF NOT EXISTS infra_name_search ON infra FIELDS name FULLTEXT ANALYZER name_analyzer BM25;
        DEFINE INDEX IF NOT EXISTS skill_name_search ON skill FIELDS name FULLTEXT ANALYZER name_analyzer BM25;

        -- === KNOWLEDGE GRAPH TABLES ===
        -- General-purpose knowledge entities beyond code: concepts, people,
        -- orgs, technologies, sources, claims. These live alongside code
        -- entities in the same graph, enabling cross-domain queries.

        DEFINE TABLE IF NOT EXISTS knowledge SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS title ON knowledge TYPE string;
        DEFINE FIELD IF NOT EXISTS content ON knowledge TYPE string;
        DEFINE FIELD IF NOT EXISTS kind ON knowledge TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON knowledge TYPE string;
        DEFINE FIELD IF NOT EXISTS source_url ON knowledge TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS confidence ON knowledge TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS tags ON knowledge TYPE option<array>;
        DEFINE FIELD IF NOT EXISTS created_at ON knowledge TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS updated_at ON knowledge TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS embedding ON knowledge TYPE option<array>;
        DEFINE FIELD IF NOT EXISTS binary_embedding ON knowledge TYPE option<array>;
        DEFINE INDEX IF NOT EXISTS know_title ON knowledge FIELDS title;
        DEFINE INDEX IF NOT EXISTS know_kind ON knowledge FIELDS kind;
        DEFINE INDEX IF NOT EXISTS know_repo ON knowledge FIELDS repo;
        DEFINE INDEX IF NOT EXISTS know_title_search ON knowledge FIELDS title FULLTEXT ANALYZER name_analyzer BM25;
        DEFINE INDEX IF NOT EXISTS know_content_search ON knowledge FIELDS content FULLTEXT ANALYZER name_analyzer BM25;

        -- Knowledge edge tables
        DEFINE TABLE supports TYPE RELATION SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS relation ON supports TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS context ON supports TYPE option<string>;
        DEFINE TABLE contradicts TYPE RELATION SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS relation ON contradicts TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS context ON contradicts TYPE option<string>;
        DEFINE TABLE related_to TYPE RELATION SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS relation ON related_to TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS context ON related_to TYPE option<string>;

        -- === META TABLE (schema version + future runtime metadata) ===

        DEFINE TABLE IF NOT EXISTS meta SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS version ON meta TYPE option<int>;
        ",
    )
    .await?;

    // Note: the schema_version is NOT stamped here. `migrations::migrate_to_current`
    // is responsible for walking a DB from its recorded version up to SCHEMA_VERSION
    // and stamping the result. A legacy DB that has never seen this code path
    // returns 0 from `get_schema_version`, which triggers the v0 → v1 migration.
    Ok(())
}
