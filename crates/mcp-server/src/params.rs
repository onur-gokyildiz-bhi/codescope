use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SearchParams {
    /// The search query (function/class name or pattern)
    pub query: String,
    /// Maximum number of results (default: 20)
    pub limit: Option<usize>,
    /// Optional scope filter (e.g. "core::graph") to narrow memory search to a specific module
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FileEntitiesParams {
    /// Path to the file to inspect
    pub file_path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindCallersParams {
    /// Name of the function to find callers for
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FindCalleesParams {
    /// Name of the function to find callees for
    pub function_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HttpCallParams {
    /// Filter by HTTP method (GET, POST, PUT, DELETE, PATCH). If not specified, returns all HTTP calls.
    pub method: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexSkillGraphParams {
    /// Folder path containing markdown skill files (relative to codebase root)
    pub path: String,
    /// Clear existing skill data before indexing (default: false)
    pub clean: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TraverseSkillGraphParams {
    /// Skill name to start traversal from
    pub name: String,
    /// Traversal depth — how many link-hops to follow (default: 1)
    pub depth: Option<usize>,
    /// Progressive disclosure level 1-4: 1=names, 2=+links (default), 3=+sections, 4=+full content
    pub detail_level: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RawQueryParams {
    /// SurrealQL query to execute
    pub query: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexParams {
    /// Path to index (relative to codebase root)
    pub path: Option<String>,
    /// Clear existing data before indexing
    pub clean: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ImpactAnalysisParams {
    /// Name of the function to analyze impact for
    pub function_name: String,
    /// Depth of the call graph to traverse (default: 3)
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct NaturalLanguageQueryParams {
    /// Natural language question about the codebase
    pub question: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SyncHistoryParams {
    /// Path to the git repository
    pub git_path: Option<String>,
    /// Number of recent commits to sync (default: 200)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct HotspotParams {
    /// Minimum risk score threshold (default: 0)
    pub min_score: Option<i64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ChurnParams {
    /// Number of top churned files to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CouplingParams {
    /// Number of top coupled file pairs to return (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DiffReviewParams {
    /// Git ref to diff against (e.g., "main", "HEAD~3", commit hash)
    pub base_ref: String,
    /// Optional head ref (default: HEAD)
    pub head_ref: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct InitProjectParams {
    /// Repository/project name (used for DB isolation)
    pub repo: String,
    /// Path to the codebase directory
    pub path: String,
    /// Auto-index the codebase after initialization
    pub auto_index: Option<bool>,
}

// === Obsidian-like exploration tools ===

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExploreParams {
    /// Entity name to explore (function, class, config key, file path, etc.)
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ContextBundleParams {
    /// File path to get full context for
    pub file_path: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RelatedParams {
    /// Keyword to search across all entity types (code, config, docs, packages)
    pub keyword: String,
    /// Maximum results per type (default: 10)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct BacklinksParams {
    /// Entity name to find backlinks for
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct IndexConversationsParams {
    /// Path to Claude projects directory (auto-detects if not provided)
    pub project_dir: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConversationSearchParams {
    /// Search query — entity name, topic keyword, or concept
    pub query: String,
    /// Filter by type: "decision", "problem", "solution", "topic", or "all" (default)
    pub entity_type: Option<String>,
    /// Maximum results (default: 20)
    pub limit: Option<usize>,
}

// === Semantic search tools ===

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ConversationTimelineParams {
    /// Entity name (function, class, file) to search conversation history for
    pub entity_name: String,
    /// Number of days to look back (default: 30)
    pub days_back: Option<u32>,
    /// Maximum results (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EmbedParams {
    /// Embedding provider: "fastembed" (default, local), "ollama", or "openai"
    pub provider: Option<String>,
    /// Batch size for embedding generation (default: 100)
    pub batch_size: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchParams {
    /// Natural language query to search for semantically similar code
    pub query: String,
    /// Maximum results (default: 10)
    pub limit: Option<usize>,
    /// Embedding provider: "fastembed" (default), "ollama", or "openai"
    pub provider: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct RenameSymbolParams {
    /// Name of the symbol (function/class) to find all references for
    pub symbol_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SafeDeleteParams {
    /// Name of the symbol to check for safe deletion
    pub symbol_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct DeadCodeParams {
    /// Minimum function size in lines to include (default: 3, filters out trivial getters)
    pub min_lines: Option<u32>,
    /// Maximum results (default: 50)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TeamPatternsParams {
    /// Focus area: "imports", "naming", "structure", or "all" (default)
    pub focus: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditPreflightParams {
    /// File path being edited
    pub file_path: String,
    /// Name of the function/class being added or modified
    pub entity_name: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ManageAdrParams {
    /// Action: "list", "create", "get"
    pub action: String,
    /// ADR title (for create)
    pub title: Option<String>,
    /// ADR body/decision text (for create)
    pub body: Option<String>,
    /// ADR ID (for get)
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct TypeHierarchyParams {
    /// Name of the class, struct, trait, or interface
    pub name: String,
    /// Depth of hierarchy traversal (default: 3, max: 5)
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GenerateSkillNotesParams {
    /// Output directory for generated skill notes (relative to codebase root, default: "skills")
    pub output_dir: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CommunityDetectionParams {
    /// Analysis type: "clusters", "bridges", "central", or "all" (default)
    pub analysis: Option<String>,
    /// Maximum results per analysis (default: 20)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CodeSmellParams {
    /// Maximum results per category (default: 10)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct CustomLintParams {
    /// SurrealQL query that returns violations (e.g. SELECT name, file_path FROM `function` WHERE ...)
    pub rule: String,
    /// Human-readable description of what this rule checks
    pub description: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ApiChangelogParams {
    /// Number of hours to look back (default: 24)
    pub hours: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ExportObsidianParams {
    /// Output directory (default: ~/.codescope/exports/{repo})
    pub output_dir: Option<String>,
    /// Maximum number of entities to export (default: 500)
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct MemoryPinParams {
    /// The decision/memory name to find (partial match)
    pub name: String,
    /// Tier level: 0 = critical (always show), 1 = important, 2 = contextual (default)
    pub tier: u32,
}
