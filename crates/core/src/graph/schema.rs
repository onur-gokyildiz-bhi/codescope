use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

/// Initialize the SurrealDB schema for the knowledge graph
pub async fn init_schema(db: &Surreal<Db>) -> Result<()> {
    db.query(
        "
        -- === CODE ENTITY TABLES ===

        DEFINE TABLE IF NOT EXISTS file SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS path ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS hash ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS line_count ON file TYPE option<int>;
        DEFINE INDEX IF NOT EXISTS file_path ON file FIELDS path;
        DEFINE INDEX IF NOT EXISTS file_repo ON file FIELDS repo;

        DEFINE TABLE IF NOT EXISTS function SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON function TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON function TYPE string;
        DEFINE FIELD IF NOT EXISTS signature ON function TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS body_hash ON function TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS file_path ON function TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON function TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON function TYPE string;
        DEFINE FIELD IF NOT EXISTS start_line ON function TYPE int;
        DEFINE FIELD IF NOT EXISTS end_line ON function TYPE int;
        DEFINE FIELD IF NOT EXISTS complexity ON function TYPE option<int>;
        DEFINE FIELD IF NOT EXISTS embedding ON function TYPE option<array>;
        DEFINE INDEX IF NOT EXISTS fn_name ON function FIELDS name;
        DEFINE INDEX IF NOT EXISTS fn_qname ON function FIELDS qualified_name UNIQUE;
        DEFINE INDEX IF NOT EXISTS fn_file ON function FIELDS file_path;

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

        -- === EDGE TABLES ===

        DEFINE TABLE IF NOT EXISTS contains SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS calls SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS line ON calls TYPE option<int>;
        DEFINE TABLE IF NOT EXISTS imports SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS inherits SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS implements SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS uses SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS modified_in SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS commit_hash ON modified_in TYPE option<string>;
        DEFINE FIELD IF NOT EXISTS timestamp ON modified_in TYPE option<datetime>;
        DEFINE FIELD IF NOT EXISTS change_type ON modified_in TYPE option<string>;
        DEFINE TABLE IF NOT EXISTS depends_on SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS configures SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS defines_endpoint SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS has_field SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS references SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS depends_on_package SCHEMAFULL;
        DEFINE TABLE IF NOT EXISTS runs_script SCHEMAFULL;
        ",
    )
    .await?;

    Ok(())
}
