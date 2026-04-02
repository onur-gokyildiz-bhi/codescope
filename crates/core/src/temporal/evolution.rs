use serde::{Deserialize, Serialize};

/// Tracks how a code entity evolves over time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityEvolution {
    pub entity_name: String,
    pub file_path: String,
    pub snapshots: Vec<EvolutionSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionSnapshot {
    pub commit_hash: String,
    pub timestamp: i64,
    pub author: String,
    pub change_type: String,
    pub body_hash: Option<String>,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub size_delta: i32,
}

// Temporal analysis will be fully implemented in Faz 3
// when we integrate git history with the graph database.
// This module provides the data structures for now.
