# SurrealQL Reference for Codescope

Quick reference for writing correct SurrealQL queries against codescope's knowledge graph.

## Reserved Words

`function` is a reserved word in SurrealQL. Always wrap it in backticks:

```sql
-- Wrong (parse error)
SELECT * FROM function WHERE name = 'main'

-- Correct
SELECT * FROM `function` WHERE name = 'main'
```

## Tables

| Table | Contains |
|-------|----------|
| `file` | Source files (path, language, line_count) |
| `` `function` `` | Functions and methods (name, file_path, start_line, end_line, signature, qualified_name) |
| `class` | Classes, structs, traits, interfaces (name, kind, file_path) |
| `import_decl` | Import/use/require statements (name, file_path) |
| `config` | Config files parsed as entities |
| `doc` | Documentation files |
| `package` | Package/module declarations |
| `infra` | Infrastructure definitions (Dockerfile, Terraform, etc.) |

## Edge Tables

| Edge | From → To | Meaning |
|------|-----------|---------|
| `calls` | function → function | Function A calls function B |
| `contains` | file → function/class | File contains entity |
| `imports` | file → import_decl | File imports module |
| `inherits` | class → class | Class A extends class B |

## Graph Traversal Syntax

### CRITICAL: chain hops directly, NO dots between hops

```sql
-- Correct: hops chain directly
SELECT <-calls<-`function`.name FROM `function` WHERE name = 'target'

-- WRONG: dot between hops is a PARSE ERROR
SELECT <-calls<-`function`.<-calls<-`function`.name FROM `function`

-- Correct: multi-hop chains
SELECT <-calls<-`function`<-calls<-`function`.name AS hop2 FROM `function` WHERE name = 'target'
SELECT <-calls<-`function`<-calls<-`function`<-calls<-`function`.name AS hop3 FROM `function` WHERE name = 'target'
```

The `.` is ONLY for the final field projection (e.g., `.name`, `.file_path`).

### Direction

| Syntax | Direction | Meaning |
|--------|-----------|---------|
| `->calls->` | Outgoing | What does this function call? |
| `<-calls<-` | Incoming | Who calls this function? |

### Examples

```sql
-- Direct callers of a function
SELECT <-calls<-`function`.name AS callers FROM `function` WHERE name = 'parse_config'

-- Direct callees (what it calls)
SELECT ->calls->`function`.name AS callees FROM `function` WHERE name = 'main'

-- 2-hop transitive callers
SELECT <-calls<-`function`<-calls<-`function`.name AS hop2
FROM `function` WHERE name = 'parse_config' LIMIT 1

-- Type hierarchy (parents + children)
SELECT name, ->inherits->class.name AS parents, <-inherits<-class.name AS children
FROM class WHERE name = 'MyStruct'

-- Fan-in: most-called functions
SELECT out.name AS name, count() AS callers
FROM calls GROUP BY out.name ORDER BY callers DESC LIMIT 10
```

## Common Queries

```sql
-- Count everything
SELECT count() FROM file GROUP ALL;
SELECT count() FROM `function` GROUP ALL;
SELECT count() FROM calls GROUP ALL

-- Search functions by partial name (case-insensitive)
SELECT name, file_path, start_line FROM `function`
WHERE string::contains(string::lowercase(name), 'parse')
LIMIT 20

-- Largest functions by line count
SELECT name, file_path, (end_line - start_line) AS size
FROM `function` ORDER BY size DESC LIMIT 10

-- All entities in a file
SELECT name, start_line, end_line FROM `function`
WHERE file_path = 'src/main.rs'
ORDER BY start_line

-- Find dead code (functions with no callers)
SELECT name, file_path FROM `function`
WHERE count(<-calls) = 0
AND name != 'main'
LIMIT 20
```

## Anti-Patterns

### 1. Nested subqueries for multi-hop traversal

```sql
-- NEVER DO THIS — quadratic scan, 5+ minutes on modest repos
SELECT * FROM `function`
WHERE name IN (
    SELECT VALUE in.name FROM calls
    WHERE out.name IN (
        SELECT VALUE in.name FROM calls WHERE out.name = 'target'
    )
)
```

Use native graph traversal syntax instead (see above). It's indexed and sub-millisecond.

### 2. String interpolation of user input

```sql
-- NEVER DO THIS — SQL injection
SELECT * FROM `function` WHERE name = '{user_input}'

-- Use parameterized bindings instead
SELECT * FROM `function` WHERE name = $name
```

### 3. Forgetting backticks on `function`

Every query touching the function table MUST backtick it. If you get a cryptic parse error, check the backticks first.

## Parameterized Queries

```sql
-- Bind variables with $
SELECT * FROM `function` WHERE name = $name
SELECT <-calls<-`function`.name FROM `function` WHERE name = $target
```

In Rust: `db.query(surql).bind(("name", value)).await`
