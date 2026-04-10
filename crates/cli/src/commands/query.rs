use anyhow::Result;
use codescope_core::graph::query::GraphQuery;
use std::path::PathBuf;

use crate::db::connect_db;

pub async fn run(surql: &str, repo: &str, db_path: Option<PathBuf>) -> Result<()> {
    let db = connect_db(db_path, repo).await?;
    let gq = GraphQuery::new(db);

    let result = gq.raw_query(surql).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
