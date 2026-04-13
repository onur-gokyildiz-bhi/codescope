//! MCP tool implementations split by topic.
//! Each module has its own `#[tool_router(router = X_router)]` impl block
//! that registers a subset of tools. The constructor in `server.rs` merges
//! all routers into one.

pub mod admin;
pub mod adr;
pub mod analytics;
pub mod ask;
pub mod callgraph;
pub mod contributors;
pub mod conversations;
pub mod embeddings;
pub mod exploration;
pub mod http;
pub mod knowledge;
pub mod memory;
pub mod quality;
pub mod refactor;
pub mod search;
pub mod skills;
pub mod temporal;
