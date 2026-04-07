pub mod evolution;
pub mod git;
pub mod graph_sync;

use serde::{Deserialize, Serialize};

pub use git::GitAnalyzer;
pub use graph_sync::{HotspotEntry, TemporalGraphSync};

/// Git commit info — shared data types (git2-independent)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub author: String,
    pub timestamp: i64,
    pub message: String,
    pub files_changed: Vec<FileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change_type: ChangeType,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Deleted,
    Renamed,
}
