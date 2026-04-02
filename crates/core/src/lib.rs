pub mod parser;
pub mod graph;
pub mod embeddings;
pub mod temporal;
pub mod crossrepo;

use serde::{Deserialize, Serialize};

/// A code entity extracted from source code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeEntity {
    pub kind: EntityKind,
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub repo: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_col: u32,
    pub end_col: u32,
    pub signature: Option<String>,
    pub body: Option<String>,
    pub body_hash: Option<String>,
    pub language: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    File,
    Module,
    Function,
    Method,
    Class,
    Struct,
    Interface,
    Trait,
    Enum,
    Variable,
    Constant,
    Import,
    TypeAlias,
}

impl EntityKind {
    pub fn table_name(&self) -> &str {
        match self {
            Self::File => "file",
            Self::Module => "module",
            Self::Function | Self::Method => "function",
            Self::Class | Self::Struct => "class",
            Self::Interface | Self::Trait => "class",
            Self::Enum => "class",
            Self::Variable | Self::Constant => "variable",
            Self::Import => "import_decl",
            Self::TypeAlias => "class",
        }
    }
}

/// A relationship between two code entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRelation {
    pub kind: RelationKind,
    pub from_entity: String,
    pub to_entity: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Contains,
    Calls,
    Imports,
    Inherits,
    Implements,
    Uses,
    ModifiedIn,
    DependsOn,
}

impl RelationKind {
    pub fn table_name(&self) -> &str {
        match self {
            Self::Contains => "contains",
            Self::Calls => "calls",
            Self::Imports => "imports",
            Self::Inherits => "inherits",
            Self::Implements => "implements",
            Self::Uses => "uses",
            Self::ModifiedIn => "modified_in",
            Self::DependsOn => "depends_on",
        }
    }
}

/// Result of indexing a codebase
#[derive(Debug, Default)]
pub struct IndexResult {
    pub files_processed: usize,
    pub entities_extracted: usize,
    pub relations_created: usize,
    pub errors: Vec<String>,
}
