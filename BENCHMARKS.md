# Codescope Benchmarks

Benchmarks run on real-world open-source Rust projects. All measurements taken on Windows 11, Rust 1.91.1, SurrealDB embedded (RocksDB).

## Token Savings vs Traditional Approach

The core value proposition: instead of reading entire source files to understand code, Codescope returns precise graph query results.

### ripgrep (101 files, 1.8 MB source)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 148,100 tokens | 542 tokens | **99.6%** |
| List all structs in project | 454,500 tokens | 1,200 tokens | **99.7%** |
| Find largest functions | 454,500 tokens | 289 tokens | **99.9%** |
| Impact analysis (callers + callees) | 186,900 tokens | 50 tokens | **~100%** |

### axum (296 files, 1.3 MB source)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 51,500 tokens | 1,200 tokens | **97.7%** |
| List all structs in project | 335,000 tokens | 1,300 tokens | **99.6%** |
| Find largest functions | 335,000 tokens | 292 tokens | **99.9%** |
| Impact analysis (callers + callees) | 72,800 tokens | 1,300 tokens | **98.2%** |

### tokio (769 files, 5.6 MB source)

| Scenario | Traditional | Codescope | Saving |
|----------|------------|-----------|--------|
| Find function + understand context | 97,800 tokens | 667 tokens | **99.3%** |
| List all structs in project | 1,400,000 tokens | 1,200 tokens | **99.9%** |
| Find largest functions | 1,400,000 tokens | 288 tokens | **~100%** |
| Impact analysis (callers + callees) | 142,400 tokens | 278 tokens | **99.8%** |

## Indexing Performance

| Repository | Files | Entities | Relations | Time | Speed |
|-----------|-------|----------|-----------|------|-------|
| ripgrep | 101 | 3,594 | 15,551 | 9.6s | 10.6 files/s |
| axum | 296 | 4,231 | 14,143 | 10.7s | 27.6 files/s |
| tokio | 769 | 12,628 | 43,755 | 33.3s | 23.1 files/s |

## Query Performance

Average query response times across all repositories:

| Query Type | ripgrep | axum | tokio |
|-----------|---------|------|-------|
| search_functions | 36.0ms | 47.3ms | 19.3ms |
| all_structs | 4.0ms | 4.7ms | 6.2ms |
| largest_functions | 122.7ms | 114.7ms | 463.5ms |
| graph_traversal (callers) | 3.0ms | 40.5ms | 15.5ms |
| count_all | 0.6ms | 1.1ms | 2.0ms |

## Methodology

- **Traditional approach**: Estimated as reading the top N source files to answer a question (~4 bytes/token).
- **Codescope approach**: Measured as the actual JSON response size from the graph query (~4 bytes/token).
- **Indexing**: Full parse of all supported source files using tree-sitter, stored in SurrealDB embedded (RocksDB).
- **Queries**: Cold-start (no caching), measured with `std::time::Instant`.

## Reproduce

```bash
git clone https://github.com/onur-gokyildiz-bhi/codescope
cd codescope
cargo run -p codescope-bench -- /path/to/repo --json --output results.json
```
