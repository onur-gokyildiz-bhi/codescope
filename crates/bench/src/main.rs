use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;

use codescope_core::graph::builder::GraphBuilder;
use codescope_core::graph::query::GraphQuery;
use codescope_core::graph::schema;
use codescope_core::parser::CodeParser;

#[derive(Parser)]
#[command(name = "codescope-bench")]
#[command(about = "Codescope Benchmark Suite")]
struct Args {
    /// Path to the codebase to benchmark
    path: PathBuf,

    /// Repository name (default: directory name)
    #[arg(long)]
    repo: Option<String>,

    /// Output results as JSON
    #[arg(long)]
    json: bool,

    /// Output file for results
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BenchmarkResults {
    repo_name: String,
    repo_path: String,

    // Index metrics
    index: IndexMetrics,

    // Query metrics
    queries: Vec<QueryMetric>,

    // Token savings
    token_savings: Vec<TokenSavingScenario>,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexMetrics {
    total_files: usize,
    supported_files: usize,
    entities_extracted: usize,
    relations_created: usize,
    index_time_ms: u128,
    files_per_second: f64,
    entities_per_second: f64,
    total_source_bytes: u64,
    db_size_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct QueryMetric {
    name: String,
    query: String,
    time_ms: f64,
    result_count: usize,
    response_tokens: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenSavingScenario {
    scenario: String,
    traditional_tokens: usize,
    codescope_tokens: usize,
    saving_percent: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    let repo_name = args.repo.unwrap_or_else(|| {
        args.path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into())
    });

    println!("=== Codescope Benchmark Suite ===\n");
    println!("Repository: {} ({})\n", repo_name, args.path.display());

    // --- Phase 1: Count source files and bytes ---
    println!("[1/4] Scanning source files...");
    let parser = CodeParser::new();
    let (total_files, supported_files, total_bytes) = count_files(&args.path, &parser);
    println!("  Total files: {}, Supported: {}, Source bytes: {}\n",
        total_files, supported_files, format_bytes(total_bytes));

    // --- Phase 2: Index benchmark ---
    println!("[2/4] Indexing benchmark...");
    let db_path = std::env::temp_dir().join(format!("codescope-bench-{}", repo_name));
    let _ = std::fs::remove_dir_all(&db_path); // Clean start

    let db = surrealdb::Surreal::new::<surrealdb::engine::local::RocksDb>(
        db_path.to_string_lossy().as_ref()
    ).await?;
    db.use_ns("bench").use_db("code").await?;
    schema::init_schema(&db).await?;

    let builder = GraphBuilder::new(db.clone());
    let start = Instant::now();

    let mut entities_total = 0usize;
    let mut relations_total = 0usize;
    let mut files_indexed = 0usize;

    let walker = ignore::WalkBuilder::new(&args.path)
        .hidden(true)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) { continue }
        let file_path = entry.path();
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !parser.supports_extension(ext) { continue }

        if let Ok((entities, relations)) = parser.parse_file(file_path, &repo_name) {
            entities_total += entities.len();
            relations_total += relations.len();
            let _ = builder.insert_entities(&entities).await;
            let _ = builder.insert_relations(&relations).await;
            files_indexed += 1;
        }
    }

    let index_time = start.elapsed();
    let files_per_sec = files_indexed as f64 / index_time.as_secs_f64();
    let entities_per_sec = entities_total as f64 / index_time.as_secs_f64();

    // Get DB size
    let db_size = dir_size(&db_path);

    let index_metrics = IndexMetrics {
        total_files,
        supported_files,
        entities_extracted: entities_total,
        relations_created: relations_total,
        index_time_ms: index_time.as_millis(),
        files_per_second: files_per_sec,
        entities_per_second: entities_per_sec,
        total_source_bytes: total_bytes,
        db_size_bytes: db_size,
    };

    println!("  Files indexed:     {}", files_indexed);
    println!("  Entities:          {}", entities_total);
    println!("  Relations:         {}", relations_total);
    println!("  Time:              {:.1}ms", index_time.as_millis());
    println!("  Speed:             {:.1} files/sec, {:.1} entities/sec", files_per_sec, entities_per_sec);
    println!("  DB size:           {}\n", format_bytes(db_size));

    // --- Phase 3: Query benchmarks ---
    println!("[3/4] Query benchmarks...");
    let gq = GraphQuery::new(db.clone());

    let queries: Vec<(&str, &str)> = vec![
        ("search_functions", "SELECT name, file_path, start_line FROM `function` WHERE string::contains(string::lowercase(name), 'new') LIMIT 20"),
        ("find_function_exact", "SELECT * FROM `function` WHERE name = 'main' LIMIT 5"),
        ("file_entities", "SELECT name, start_line, end_line FROM `function` ORDER BY file_path LIMIT 30"),
        ("all_structs", "SELECT name, file_path FROM class ORDER BY name LIMIT 50"),
        ("largest_functions", "SELECT name, file_path, (end_line - start_line) AS size FROM `function` ORDER BY size DESC LIMIT 10"),
        ("graph_traversal_callers", "SELECT <-calls<-`function`.name AS callers FROM `function` WHERE name = 'main'"),
        ("graph_traversal_callees", "SELECT ->calls->`function`.name AS callees FROM `function` WHERE name = 'main'"),
        ("count_all", "SELECT count() FROM file GROUP ALL"),
        ("imports_list", "SELECT name, file_path FROM import_decl LIMIT 30"),
    ];

    let mut query_metrics = Vec::new();

    for (name, surql) in &queries {
        let start = Instant::now();
        let result = gq.raw_query(surql).await;
        let elapsed = start.elapsed();

        let (result_count, response_tokens) = match &result {
            Ok(v) => {
                let json = serde_json::to_string(v).unwrap_or_default();
                let count = v.as_array().map(|a| a.len()).unwrap_or(0);
                let tokens = json.len() / 4; // rough estimate
                (count, tokens)
            }
            Err(_) => (0, 0),
        };

        println!("  {:<30} {:>8.2}ms  ({} results, ~{} tokens)",
            name, elapsed.as_secs_f64() * 1000.0, result_count, response_tokens);

        query_metrics.push(QueryMetric {
            name: name.to_string(),
            query: surql.to_string(),
            time_ms: elapsed.as_secs_f64() * 1000.0,
            result_count,
            response_tokens,
        });
    }

    // --- Phase 4: Token savings estimation ---
    println!("\n[4/4] Token savings analysis...");

    let mut scenarios = Vec::new();

    // Scenario 1: "Find a function and understand its context"
    let traditional_1 = estimate_traditional_tokens_for_search(&args.path, &parser, 5);
    let codescope_1 = query_metrics.iter()
        .filter(|q| q.name == "search_functions" || q.name == "graph_traversal_callers")
        .map(|q| q.response_tokens)
        .sum::<usize>()
        .max(50);
    scenarios.push(token_scenario(
        "Find function + understand context",
        traditional_1, codescope_1,
    ));

    // Scenario 2: "List all structs/classes"
    let traditional_2 = (total_bytes as usize) / 4; // read all files
    let codescope_2 = query_metrics.iter()
        .find(|q| q.name == "all_structs")
        .map(|q| q.response_tokens)
        .unwrap_or(100);
    scenarios.push(token_scenario(
        "List all structs in project",
        traditional_2, codescope_2,
    ));

    // Scenario 3: "Largest/most complex functions"
    let traditional_3 = (total_bytes as usize) / 4;
    let codescope_3 = query_metrics.iter()
        .find(|q| q.name == "largest_functions")
        .map(|q| q.response_tokens)
        .unwrap_or(100);
    scenarios.push(token_scenario(
        "Find largest functions",
        traditional_3, codescope_3,
    ));

    // Scenario 4: "Impact analysis — who calls this?"
    let traditional_4 = estimate_traditional_tokens_for_search(&args.path, &parser, 8);
    let codescope_4 = query_metrics.iter()
        .filter(|q| q.name.contains("callers") || q.name.contains("callees"))
        .map(|q| q.response_tokens)
        .sum::<usize>()
        .max(50);
    scenarios.push(token_scenario(
        "Impact analysis (callers + callees)",
        traditional_4, codescope_4,
    ));

    println!();
    println!("  {:<45} {:>12} {:>12} {:>10}",
        "Scenario", "Traditional", "Codescope", "Saving");
    println!("  {}", "-".repeat(83));
    for s in &scenarios {
        println!("  {:<45} {:>12} {:>12} {:>9.1}%",
            s.scenario,
            format_tokens(s.traditional_tokens),
            format_tokens(s.codescope_tokens),
            s.saving_percent,
        );
    }

    // Build final results
    let results = BenchmarkResults {
        repo_name: repo_name.clone(),
        repo_path: args.path.to_string_lossy().to_string(),
        index: index_metrics,
        queries: query_metrics,
        token_savings: scenarios,
    };

    // Output
    if args.json || args.output.is_some() {
        let json = serde_json::to_string_pretty(&results)?;
        if let Some(output) = &args.output {
            std::fs::write(output, &json)?;
            println!("\nResults written to {}", output.display());
        } else {
            println!("\n{}", json);
        }
    }

    // Cleanup temp DB
    let _ = std::fs::remove_dir_all(&db_path);

    println!("\nBenchmark complete.");
    Ok(())
}

fn count_files(path: &PathBuf, parser: &CodeParser) -> (usize, usize, u64) {
    let mut total = 0;
    let mut supported = 0;
    let mut bytes = 0u64;

    let walker = ignore::WalkBuilder::new(path)
        .hidden(true)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) { continue }

        total += 1;
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        if parser.supports_extension(ext) {
            supported += 1;
            bytes += entry.path().metadata().map(|m| m.len()).unwrap_or(0);
        }
    }

    (total, supported, bytes)
}

fn estimate_traditional_tokens_for_search(path: &PathBuf, parser: &CodeParser, files_to_read: usize) -> usize {
    let mut sizes = Vec::new();

    let walker = ignore::WalkBuilder::new(path)
        .hidden(true)
        .git_ignore(true)
        .build();

    for entry in walker {
        let entry = match entry { Ok(e) => e, Err(_) => continue };
        if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) { continue }
        let ext = entry.path().extension().and_then(|e| e.to_str()).unwrap_or("");
        if parser.supports_extension(ext) {
            let size = entry.path().metadata().map(|m| m.len() as usize).unwrap_or(0);
            sizes.push(size);
        }
    }

    sizes.sort_unstable_by(|a, b| b.cmp(a));
    let top_bytes: usize = sizes.iter().take(files_to_read).sum();
    top_bytes / 4 // ~4 bytes per token
}

fn token_scenario(name: &str, traditional: usize, codescope: usize) -> TokenSavingScenario {
    let saving = if traditional > 0 {
        (1.0 - (codescope as f64 / traditional as f64)) * 100.0
    } else {
        0.0
    };
    TokenSavingScenario {
        scenario: name.to_string(),
        traditional_tokens: traditional,
        codescope_tokens: codescope,
        saving_percent: saving,
    }
}

fn format_bytes(b: u64) -> String {
    if b >= 1_000_000 { format!("{:.1} MB", b as f64 / 1_000_000.0) }
    else if b >= 1_000 { format!("{:.1} KB", b as f64 / 1_000.0) }
    else { format!("{} B", b) }
}

fn format_tokens(t: usize) -> String {
    if t >= 1_000_000 { format!("{:.1}M", t as f64 / 1_000_000.0) }
    else if t >= 1_000 { format!("{:.1}K", t as f64 / 1_000.0) }
    else { format!("{}", t) }
}

fn dir_size(path: &PathBuf) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}
