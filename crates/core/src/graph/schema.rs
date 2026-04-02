use anyhow::Result;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

/// Initialize the SurrealDB schema for code graph storage
pub async fn init_schema(db: &Surreal<Db>) -> Result<()> {
    // Node tables
    db.query(
        "
        -- File nodes
        DEFINE TABLE IF NOT EXISTS file SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS path ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS language ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS hash ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON file TYPE string;
        DEFINE FIELD IF NOT EXISTS line_count ON file TYPE option<int>;
        DEFINE INDEX IF NOT EXISTS file_path ON file FIELDS path;
        DEFINE INDEX IF NOT EXISTS file_repo ON file FIELDS repo;

        -- Function/method nodes
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

        -- Class/struct/interface nodes
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

        -- Module nodes
        DEFINE TABLE IF NOT EXISTS module SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON module TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON module TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON module TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON module TYPE string;
        DEFINE INDEX IF NOT EXISTS mod_qname ON module FIELDS qualified_name UNIQUE;

        -- Variable nodes
        DEFINE TABLE IF NOT EXISTS variable SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON variable TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON variable TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON variable TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON variable TYPE string;
        DEFINE INDEX IF NOT EXISTS var_qname ON variable FIELDS qualified_name UNIQUE;

        -- Import declaration nodes
        DEFINE TABLE IF NOT EXISTS import_decl SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS name ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS qualified_name ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS file_path ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS repo ON import_decl TYPE string;
        DEFINE FIELD IF NOT EXISTS body ON import_decl TYPE option<string>;
        DEFINE INDEX IF NOT EXISTS imp_qname ON import_decl FIELDS qualified_name UNIQUE;

        -- Edge tables
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
        ",
    )
    .await?;

    Ok(())
}
