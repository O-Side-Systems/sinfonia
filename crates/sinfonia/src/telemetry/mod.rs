//! Observability layer.
//!
//! `init_observability` is the single subscriber-setup entry point for the
//! Sinfonia daemon. It wraps today's `tracing_subscriber::fmt` layer with an
//! optional OTel exporter layer when a `telemetry:` block is configured in
//! WORKFLOW.md. When no endpoint is configured (and no
//! `OTEL_EXPORTER_OTLP_ENDPOINT` env var is set), the OTel layer is `None`
//! and behavior matches stdout-only logging — the feature is opt-in by
//! configuration.
//!
//! Crate set in use: `opentelemetry 0.32` / `opentelemetry_sdk 0.32` /
//! `opentelemetry-otlp 0.32` / `tracing-opentelemetry 0.33` (the tracing
//! bridge tracks one minor ahead of the OTel core it wraps). The
//! `with_batch_exporter` call is the 0.32-API form (exporter only;
//! the batch processor picks the runtime from `opentelemetry_sdk`'s
//! features). Pattern in `build_otel_layer` below.

pub mod spans;
pub mod tenant;

pub use tenant::{TenantId, DEFAULT_TENANT, TENANT_ENV_VAR};

use crate::config::TelemetryConfig;
use opentelemetry::{global, trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use opentelemetry_semantic_conventions::resource as semconv;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Returned by `init_observability` so the runtime can `shutdown()` the
/// tracer provider on drop, guaranteeing buffered spans are flushed before
/// process exit. The bridge holds an equivalent guard.
#[must_use = "drop the guard at end-of-main so OTel can flush"]
pub struct ObservabilityGuard {
    provider: Option<SdkTracerProvider>,
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            // Best-effort shutdown — the OTel SDK already logs internally on
            // failure. We don't want a crashing shutdown path to mask the
            // original error a binary is exiting on.
            let _ = provider.shutdown();
        }
    }
}

/// Initialize the global tracing subscriber with stdout + optional OTel
/// layers. Idempotent at the subscriber level (the underlying
/// `tracing_subscriber::registry().init()` panics on second call) — callers
/// should call this exactly once during process startup, after config parse.
pub fn init_observability(format: &str, telemetry: &TelemetryConfig) -> ObservabilityGuard {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let stdout_layer = if format == "json" {
        tracing_subscriber::fmt::layer().json().boxed()
    } else {
        tracing_subscriber::fmt::layer().boxed()
    };

    let (otel_layer, provider) = build_otel_layer(telemetry);

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer);

    match otel_layer {
        Some(otel) => registry.with(otel).init(),
        None => registry.init(),
    }

    ObservabilityGuard { provider }
}

/// Build the OTel layer when the user has configured an exporter. Returns
/// `(None, None)` when no exporter is configured — the binary then runs
/// stdout-only just like today.
fn build_otel_layer<S>(
    telemetry: &TelemetryConfig,
) -> (
    Option<Box<dyn Layer<S> + Send + Sync + 'static>>,
    Option<SdkTracerProvider>,
)
where
    S: tracing::Subscriber + Send + Sync + 'static,
    for<'span> S: tracing_subscriber::registry::LookupSpan<'span>,
{
    // Effective endpoint: explicit config wins; otherwise let the SDK fall
    // back to OTEL_EXPORTER_OTLP_ENDPOINT itself. Returning None when
    // neither is set keeps the binary's defaults intact.
    let endpoint = telemetry
        .otlp_endpoint
        .clone()
        .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
        .filter(|s| !s.trim().is_empty());

    let Some(endpoint) = endpoint else {
        return (None, None);
    };

    // Headers (Honeycomb / Datadog API keys, etc.) are handed off to the
    // OTel SDK via the standard `OTEL_EXPORTER_OTLP_HEADERS` env var.
    // Carrying them in the YAML block AND populating the env var here means
    // the OTel client picks them up over both transports (gRPC + HTTP)
    // without us reaching into transport-specific metadata APIs. We only
    // set the env var when it isn't already set, so an operator can still
    // override from the shell.
    if !telemetry.headers.is_empty() && std::env::var_os("OTEL_EXPORTER_OTLP_HEADERS").is_none()
    {
        let joined: Vec<String> = telemetry
            .headers
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        std::env::set_var("OTEL_EXPORTER_OTLP_HEADERS", joined.join(","));
    }

    let exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&endpoint)
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            // Tracing isn't initialized yet — surface to stderr and fall
            // back to no-otel so the binary still starts. Mirrors what
            // operators expect when a Collector is briefly unreachable.
            eprintln!(
                "telemetry: OTLP exporter init failed ({err}); continuing without OTel"
            );
            return (None, None);
        }
    };

    let resource = build_resource(telemetry);
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();
    global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer(telemetry.service_name.clone());
    let layer: Box<dyn Layer<S> + Send + Sync + 'static> = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .boxed();

    (Some(layer), Some(provider))
}

/// Resource-level attributes (plan §3.3). The OTel routing-processor splits
/// per-tenant by `service.namespace`, so the resolved tenant id lands there
/// as well as on every span via the orchestrator's `tenant_id` field.
fn build_resource(telemetry: &TelemetryConfig) -> Resource {
    let tenant = telemetry.tenant_id.as_str().to_string();
    Resource::builder()
        .with_service_name(telemetry.service_name.clone())
        .with_attributes([
            KeyValue::new(semconv::SERVICE_NAMESPACE, tenant),
            KeyValue::new(semconv::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(
                semconv::SERVICE_INSTANCE_ID,
                uuid::Uuid::new_v4().to_string(),
            ),
        ])
        .build()
}
