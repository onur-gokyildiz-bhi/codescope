//! Tests for CUDA semantic parsing:
//! - `.cu` / `.cuh` / `.cu.inc` file-extension recognition
//! - `__global__` / `__device__` / `__host__` qualifier capture
//! - `<<<grid, block>>>` kernel launch detection

use codescope_core::parser::CodeParser;
use codescope_core::{EntityKind, RelationKind};
use std::path::Path;

const CUDA_SAMPLE: &str = r#"
#include <cuda_runtime.h>

__device__ int add_device(int a, int b) {
    return a + b;
}

__host__ int add_host(int a, int b) {
    return a + b;
}

__global__ void vector_add(const float* a, const float* b, float* c, int n) {
    int i = blockIdx.x * blockDim.x + threadIdx.x;
    if (i < n) c[i] = a[i] + b[i];
}

void launch(const float* a, const float* b, float* c, int n) {
    vector_add<<<(n + 255) / 256, 256>>>(a, b, c, n);
}
"#;

#[test]
fn cuda_file_extension_is_supported() {
    let parser = CodeParser::default();
    assert!(parser.supports_extension("cu"), "`.cu` must be supported");
    assert!(parser.supports_extension("cuh"), "`.cuh` must be supported");
}

#[test]
fn cuda_qualifier_is_captured_on_functions() {
    let parser = CodeParser::default();
    let (entities, _) = parser
        .parse_source(Path::new("sample.cu"), CUDA_SAMPLE, "test")
        .expect("parse_source for CUDA file");

    let fns: Vec<_> = entities
        .iter()
        .filter(|e| matches!(e.kind, EntityKind::Function))
        .collect();

    assert!(
        fns.iter()
            .any(|f| f.name == "vector_add" && f.cuda_qualifier.as_deref() == Some("__global__")),
        "__global__ kernel should carry qualifier; got: {:?}",
        fns.iter()
            .map(|f| (&f.name, &f.cuda_qualifier))
            .collect::<Vec<_>>()
    );
    assert!(
        fns.iter()
            .any(|f| f.name == "add_device" && f.cuda_qualifier.as_deref() == Some("__device__")),
        "__device__ helper should carry qualifier"
    );
    assert!(
        fns.iter()
            .any(|f| f.name == "add_host" && f.cuda_qualifier.as_deref() == Some("__host__")),
        "__host__ helper should carry qualifier"
    );
    // Non-CUDA function has no qualifier.
    assert!(
        fns.iter()
            .any(|f| f.name == "launch" && f.cuda_qualifier.is_none()),
        "plain host function should NOT carry a qualifier"
    );
}

#[test]
fn kernel_launch_creates_calls_relation() {
    let parser = CodeParser::default();
    let (_, relations) = parser
        .parse_source(Path::new("sample.cu"), CUDA_SAMPLE, "test")
        .expect("parse_source for CUDA file");

    let launches: Vec<_> = relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .filter(|r| r.to_entity.ends_with(":vector_add"))
        .collect();

    assert!(
        !launches.is_empty(),
        "kernel launch must emit a Calls relation targeting `vector_add`"
    );
    // At least one launch should be tagged with kernel_launch metadata.
    assert!(
        launches.iter().any(|r| r
            .metadata
            .as_ref()
            .and_then(|m| m.get("kind"))
            .and_then(|k| k.as_str())
            == Some("kernel_launch")),
        "kernel launch metadata should carry `kind: kernel_launch`; got: {:?}",
        launches.iter().map(|r| &r.metadata).collect::<Vec<_>>()
    );
}

#[test]
fn cuda_double_extension_inc_is_recognized() {
    let parser = CodeParser::default();
    // `Path::extension()` returns `"inc"` for `kernel.cu.inc`, so we rely on
    // the filename-based remapping in parse_source.
    let src = "__global__ void noop() {}\n";
    let result = parser.parse_source(Path::new("kernel.cu.inc"), src, "test");
    assert!(
        result.is_ok(),
        "parse_source for .cu.inc should succeed: {:?}",
        result.err()
    );
    let (entities, _) = result.unwrap();
    assert!(
        entities
            .iter()
            .any(|e| e.name == "noop" && e.cuda_qualifier.as_deref() == Some("__global__")),
        "__global__ kernel in .cu.inc should be parsed as CUDA"
    );
}
