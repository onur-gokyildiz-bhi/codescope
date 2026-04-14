//! OpenTelemetry (OTLP) observability for the MCP server.
//!
//! Exports traces for MCP tool invocations, graph queries, and cache-hit
//! counters so operators can see bottlenecks in Jaeger / Grafana Tempo /
//! Honeycomb, or any OTLP-compatible collector.
//!
//! This module is a strict **no-op** when `CODESCOPE_OTLP_ENDPOINT` is not
//! set — we never touch the network and never install the OTel tracer
//! provider.

use anyhow::Result;
use opentelemetry::{global, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{trace::TracerProvider, Resource};

/// Initialize the OpenTelemetry tracer provider and install it globally.
///
/// Returns:
/// - `Ok(Some(provider))` when OTLP is enabled and the pipeline installed.
///   Caller should keep the provider alive for the process lifetime, then
///   call [`shutdown`] to flush pending spans.
/// - `Ok(None)` when `CODESCOPE_OTLP_ENDPOINT` is unset/empty — strict no-op.
/// - `Err(_)` if the exporter could not be constructed (bad endpoint, etc).
pub fn init() -> Result<Option<TracerProvider>> {
    let endpoint = match std::env::var("CODESCOPE_OTLP_ENDPOINT") {
        Ok(e) if !e.is_empty() => e,
        _ => return Ok(None),
    };

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()?;

    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .with_resource(Resource::new(vec![
            KeyValue::new("service.name", "codescope-mcp"),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ]))
        .build();

    global::set_tracer_provider(provider.clone());
    Ok(Some(provider))
}

/// Flush any pending spans and drop the global tracer provider.
/// Safe to call even when telemetry was never initialized — no-op then.
pub fn shutdown() {
    global::shutdown_tracer_provider();
}
