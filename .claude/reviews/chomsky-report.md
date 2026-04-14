# Chomsky's Parser Audit — 2026-04-14

Auditor: Chomsky (parser-specialist)
Scope: `crates/core/src/parser/` — tree-sitter integration, CUDA extractor, C/C++ declarator chain, content parsers, tests.

## Language coverage

Source of truth: `crates/core/src/parser/languages.rs` (49 `LanguageConfig` entries) and `crates/core/Cargo.toml` (tree-sitter grammar deps).

**Supported: 48 tree-sitter code languages** (49 LanguageConfig entries — TypeScript + TSX both use the typescript crate, so 48 distinct grammars).

Full list, grouped:

- Mainstream: TypeScript, TSX, JavaScript, Python, Rust, Go, Java, C, C++, C#, Ruby, PHP, Swift, Dart, Scala, Lua, Zig, HTML, CSS, Bash
- Niche/scientific: Haskell, OCaml, Elixir, Erlang, Julia, R, Fortran, Ada, Common Lisp, Scheme, Racket, Elm, Groovy, Pascal
- Hardware/scientific: Verilog, GLSL, CUDA (via tree-sitter-cpp)
- Systems: D, Objective-C, Nix
- Build/config: CMake, Make, HCL/Terraform
- Web/DSL: GraphQL, XML, Protobuf
- Game: GDScript
- Blockchain: Solidity

**Special case — CUDA**: `.cu` / `.cuh` extensions are mapped to `tree_sitter_cpp::LANGUAGE` with the logical name `"cuda"` (languages.rs:86–90). `parse_source` also handles `.cu.inc` filename remapping (per `cuda_parser_tests.rs:107–124`).

**Content parsers (non-tree-sitter)**: 13 specialized parsers under `crates/core/src/parser/content/` — dockerfile, env, gradle, json, markdown, openapi, package, proto, sql, terraform, toml, yaml, plus mod.

### Pending (tree-sitter 0.26 upgrade)

Agent spec (`parser-specialist.md:37`) flagged these as pending: Kotlin, Perl, Svelte, Vue, PowerShell.

Verified by search over `Cargo.toml` and `crates/core/src/parser/`: **none of these 5 grammars are present as deps or registered in `LanguageRegistry`**. The gap identified in the spec is still open. Gradle `.gradle.kts` files are handled only at the content-parser level (`content/gradle_parser.rs`), so Kotlin code itself is not indexed.

Current tree-sitter base version across deps is a mix of 0.23–0.25 plus a few 1.x majors (`zig 1.1.2`, `fortran 0.5.1` — no, `proto 0.4`, `verilog 1.0.3`, `r 1.2`, `solidity 1.2`). Not yet uniformly on 0.26.

## CUDA support

- `__global__` / `__device__` / `__host__` detection: PRESENT — `detect_cuda_qualifier` at `extractor.rs:801–824`. Uses a 200-byte pre-node window trimmed to last `}` or `;` boundary plus the function head up to the first `(`. First-match-wins ordering `__global__ > __device__ > __host__` by declaration-order index.
- Called from `extract_function` at `extractor.rs:263–267`, gated on `self.language == "cuda"` — correctly avoids false positives on plain C++.
- Kernel launches: PRESENT — `detect_kernel_launches` at `extractor.rs:832–899`. Emits `RelationKind::Calls` edges with metadata `{line, raw_callee, kind: "kernel_launch", launch_config}`. Invoked from `extract_calls` at `extractor.rs:366–377`, gated on `self.language == "cuda"`.
- Uses byte-scan (not tree-sitter walk) — correct design per spec, since tree-sitter-cpp misparses `<<<...>>>` as bit-shifts.
- Test coverage: STRONG — `crates/core/tests/cuda_parser_tests.rs` covers:
  - `.cu` / `.cuh` extension recognition
  - `__global__` / `__device__` / `__host__` qualifier capture on `vector_add` / `add_device` / `add_host`
  - Plain host function (`launch`) correctly gets `cuda_qualifier = None`
  - Kernel launch `Calls` relation with `kind: "kernel_launch"` metadata
  - `.cu.inc` double-extension via filename remapping

## C/C++ extraction

- Declarator chain: PRESENT — `extract_c_declarator_name` at `extractor.rs:564–593`. Starts from `declarator` field, iterates up to 16 levels, terminates on `identifier` / `field_identifier` / `type_identifier` / `qualified_identifier` / `destructor_name` / `operator_name`.
- Handles nested `pointer_declarator` / `function_declarator` / `reference_declarator` / `parenthesized_declarator` / `array_declarator` by recursing via the `declarator` field, with a fallback to the first named child if no declarator field exists.
- Called from `extract_function` at `extractor.rs:234` as the third fallback after `find_child_text("name")` and `find_child_text("identifier")`. Covers C, C++, and CUDA function-definition naming.

## Red flags

1. **No dedicated C/C++ parser test fixture.** `cuda_parser_tests.rs` exercises the declarator chain implicitly for simple CUDA kernels, but there is no `cpp_parser_tests.rs` covering nasty C++ cases like `int (*const foo(int))[10]`, `T::template ns::operator<=>(...)`, `Foo::~Foo()`, pointer-to-member, ref-qualified member functions, `noexcept(...)`, trailing-return-type `auto f() -> T`, or templates. Hitting the 16-level recursion cap or the `named_child(0)` fallback silently returns wrong names. Recommend adding a targeted test suite.
2. **`build_function_signature` is a placeholder** (extractor.rs:595–600) — it returns the first line of the node text, not a proper parameter-extracted signature. Good-enough for grep/display, but won't help with overload disambiguation.
3. **CUDA window heuristic edge cases** — `detect_cuda_qualifier` trims on `}` or `;`. A `__global__` kernel following a struct/class with an inline method that contains `;` could end up with the wrong window. Also, macro-expanded `__global__` (e.g. `#define KERNEL __global__`) will be missed. No test covers macros.
4. **`tree-sitter-hcl = "1.1"` and `tree-sitter-make = "1.1"`** have no matching version check in languages.rs — if upstream changed the `LANGUAGE` const name (e.g. to `LANGUAGE_HCL`) this would be a compile break. Not a current bug but fragile.
5. **Objective-C uses `tree-sitter-objc = "3.0.2"`** — outlier at major 3. Worth sanity-checking at next fmt/clippy pass.
6. **`.h` defaulted to C** (languages.rs:68), not C++ as the spec recommends (`parser-specialist.md:38`). Headers in C++-only projects will be parsed with the C grammar, which misses templates/classes. This is a spec-vs-code divergence — either update the code or update the spec.
7. **Pending tree-sitter 0.26 grammars (Kotlin/Perl/Svelte/Vue/PowerShell)** still not added. Kotlin in particular is a common ask for Android codebases; Gradle content-parser half-handles it but actual `.kt` source is uncovered.
8. **TSX and TypeScript share the `tree-sitter-typescript` crate** — fine, but both map to the logical `language` field as `typescript` / `tsx`. If any downstream consumer expects only `"typescript"`, TSX entities could be miscategorized.
9. **48 tree-sitter grammars linked statically** — compile-time and binary size cost. No feature-gating per-language. Not a correctness issue but worth tracking for release builds.
10. **No regression fixtures checked in** for tokio / ripgrep / FastAPI as described in the agent spec ("must not reduce function extraction on existing fixtures"). Regression baseline lives only in the reviewer's head.

## Action items

1. Add `crates/core/tests/cpp_parser_tests.rs` with adversarial declarator fixtures: destructors, operators, `operator<=>`, function-returning-pointer-to-array, templates, ref-qualified methods.
2. Add a macro-style CUDA qualifier test (`#define KERNEL __global__`) and decide whether to expand handling or document the limitation.
3. Decide `.h` default grammar: either switch to cpp in `languages.rs:68` or update `parser-specialist.md:38` to reflect current C-default behavior.
4. Track the tree-sitter 0.26 migration for Kotlin/Perl/Svelte/Vue/PowerShell as a `status:planned` knowledge node. Kotlin is the highest-value add.
5. Check in a small regression fixture set (tokio, ripgrep, FastAPI snapshots) and a test that asserts minimum function counts — guards against silent regressions in `extract_c_declarator_name`, `walk_for_calls_with_entities`, or grammar bumps.
6. Consider feature-gating low-usage grammars (Ada, CommonLisp, Scheme, Racket, Pascal) behind a `full-langs` cargo feature to shrink the default binary.
7. Harden `detect_cuda_qualifier` with a test that places a `__global__` kernel immediately after an inline-method-containing class.

## Key file references

- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\Cargo.toml` — tree-sitter grammar deps
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\languages.rs` — 49 LanguageConfig entries
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\extractor.rs:234` — C declarator fallback
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\extractor.rs:263-267` — CUDA qualifier call site
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\extractor.rs:366-377` — kernel launch call site
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\extractor.rs:564-593` — `extract_c_declarator_name`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\extractor.rs:801-824` — `detect_cuda_qualifier`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\extractor.rs:832-899` — `detect_kernel_launches`
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\tests\cuda_parser_tests.rs` — CUDA test suite (4 tests)
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\tests\integration_tests.rs` — Rust/Python parser tests
- `C:\Users\onurg\OneDrive\Documents\graph-rag\crates\core\src\parser\content\` — 12 content-format parsers
