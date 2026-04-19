//! Codescope Core — code intelligence engine.
//!
//! Parses source code with tree-sitter, builds a knowledge graph in SurrealDB,
//! and provides semantic search via FastEmbed. Supports 35+ languages plus
//! config files (JSON/YAML/TOML), docs (Markdown), SQL, Terraform, and more.

#[allow(dead_code)] // Exposes fields for MCP tools and external consumers
pub mod conversation;
pub mod crossrepo;
pub mod daemon;
pub mod db;
pub mod embeddings;
pub mod gain;
pub mod graph;
pub mod insight;
pub mod parser;
pub mod temporal;

pub use db::{connect_admin, connect_path, connect_repo, DbHandle, DEFAULT_NS};

use serde::{Deserialize, Serialize};

/// An entity extracted from any content type (code, config, docs, infra, etc.)
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
    /// CUDA kernel/function qualifier: `__global__`, `__device__`, or `__host__`.
    /// Only set for functions parsed from `.cu` / `.cuh` files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cuda_qualifier: Option<String>,
}

/// Classification of extracted entities across all supported content types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    // Code entities
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

    // Config/Data entities (JSON, YAML, TOML)
    ConfigKey,
    ConfigSection,

    // Documentation entities (Markdown)
    DocSection,
    DocLink,
    DocCodeBlock,

    // API entities (OpenAPI, Protobuf)
    ApiEndpoint,
    ApiSchema,
    ApiField,

    // Database entities (SQL)
    DbTable,
    DbColumn,
    DbIndex,
    DbView,

    // Infrastructure entities (Terraform, Dockerfile, K8s)
    InfraResource,
    InfraVariable,
    InfraProvider,
    DockerStage,
    DockerInstruction,

    // Package entities (package.json, Cargo.toml)
    Package,
    Dependency,
    Script,

    // HTTP client call entities (cross-service linking)
    HttpClientCall,

    // Skill/Knowledge graph entities (arscontexta-style)
    SkillNode,
    SkillMOC,

    // Conversation entities (Claude session transcripts)
    ConversationSession,
    ConversationTopic,
    Decision,
    Problem,
    Solution,
}

impl EntityKind {
    /// Returns the SurrealDB table name for this entity kind.
    pub fn table_name(&self) -> &str {
        match self {
            // Code
            Self::File => "file",
            Self::Module => "module",
            Self::Function | Self::Method => "function",
            Self::Class
            | Self::Struct
            | Self::Interface
            | Self::Trait
            | Self::Enum
            | Self::TypeAlias => "class",
            Self::Variable | Self::Constant => "variable",
            Self::Import => "import_decl",

            // Config
            Self::ConfigKey | Self::ConfigSection => "config",

            // Documentation
            Self::DocSection | Self::DocLink | Self::DocCodeBlock => "doc",

            // API
            Self::ApiEndpoint | Self::ApiSchema | Self::ApiField => "api",

            // HTTP client calls
            Self::HttpClientCall => "http_call",

            // Skill/Knowledge graph
            Self::SkillNode | Self::SkillMOC => "skill",

            // Database
            Self::DbTable | Self::DbColumn | Self::DbIndex | Self::DbView => "db_entity",

            // Infrastructure
            Self::InfraResource
            | Self::InfraVariable
            | Self::InfraProvider
            | Self::DockerStage
            | Self::DockerInstruction => "infra",

            // Package
            Self::Package | Self::Dependency | Self::Script => "package",

            // Conversation
            Self::ConversationSession => "conversation",
            Self::ConversationTopic => "conv_topic",
            Self::Decision => "decision",
            Self::Problem => "problem",
            Self::Solution => "solution",
        }
    }
}

/// A relationship between two entities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRelation {
    pub kind: RelationKind,
    pub from_entity: String,
    pub to_entity: String,
    pub from_table: String,
    pub to_table: String,
    pub metadata: Option<serde_json::Value>,
}

/// Classification of edges in the knowledge graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    // Code relations
    Contains,
    Calls,
    Imports,
    Inherits,
    Implements,
    Uses,
    ModifiedIn,
    DependsOn,

    // Config/Doc/API/Infra relations
    Configures,
    DefinesEndpoint,
    HasField,
    References,
    DependsOnPackage,
    RunsScript,

    // HTTP cross-service relations
    CallsEndpoint,

    // Skill graph wikilink relations
    LinksTo,

    // Conversation relations
    DiscussedIn,
    DecidedAbout,
    SolvesFor,
    CoDiscusses,
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
            Self::Configures => "configures",
            Self::DefinesEndpoint => "defines_endpoint",
            Self::HasField => "has_field",
            Self::References => "references",
            Self::DependsOnPackage => "depends_on_package",
            Self::RunsScript => "runs_script",
            Self::CallsEndpoint => "calls_endpoint",
            Self::LinksTo => "links_to",
            Self::DiscussedIn => "discussed_in",
            Self::DecidedAbout => "decided_about",
            Self::SolvesFor => "solves_for",
            Self::CoDiscusses => "co_discusses",
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
