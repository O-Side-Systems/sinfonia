//! Bridge-side observability (plan §2, §3). Sibling of
//! `sinfonia::telemetry`; same shape, different input type (`TelemetrySection`
//! parsed from BRIDGE.md vs `TelemetryConfig` from WORKFLOW.md) and a
//! different default `service.name`.
//!
//! See `sinfonia::telemetry` for the rationale on:
//!
//! - the OTel 0.32 / `tracing-opentelemetry` 0.33 version set;
//! - the API delta vs. the plan-doc snippet (`SdkTracerProvider`, not
//!   `TracerProvider`);
//! - the env-var route for HTTP/gRPC headers
//!   (`OTEL_EXPORTER_OTLP_HEADERS`).

pub mod spans;
pub mod tenant;

pub use tenant::{TenantId, DEFAULT_TENANT, TENANT_ENV_VAR};

use crate::config::TelemetrySection;
use opentelemetry::{global, trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{trace::SdkTracerProvider, Resource};
use opentelemetry_semantic_conventions::resource as semconv;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Resolved tenant + tracer-provider guard. Held by the bridge's `main`
/// across the listener's lifetime; the `Drop` impl flushes buffered spans.
#[must_use = "drop the guard at end-of-main so OTel can flush"]
pub struct ObservabilityGuard {
    pub tenant_id: TenantId,
    provider: Option<SdkTracerProvider>,
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            let _ = provider.shutdown();
        }
    }
}

pub fn init_observability(format: &str, telemetry: &TelemetrySection) -> ObservabilityGuard {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let stdout_layer = if format == "json" {
        tracing_subscriber::fmt::layer().json().boxed()
    } else {
        tracing_subscriber::fmt::layer().boxed()
    };

    let tenant_id = TenantId::resolve(telemetry.tenant_id.as_deref());
    let (otel_layer, provider) = build_otel_layer(telemetry, &tenant_id);

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer);

    match otel_layer {
        Some(otel) => registry.with(otel).init(),
        None => registry.init(),
    }

    ObservabilityGuard {
        tenant_id,
        provider,
    }
}

fn build_otel_layer<S>(
    telemetry: &TelemetrySection,
    tenant_id: &TenantId,
) -> (
    Option<Box<dyn Layer<S> + Send + Sync + 'static>>,
    Option<SdkTracerProvider>,
)
where
    S: tracing::Subscriber + Send + Sync + 'static,
    for<'span> S: tracing_subscriber::registry::LookupSpan<'span>,
{
    let endpoint = telemetry
        .otlp_endpoint
        .clone()
        .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
        .filter(|s| !s.trim().is_empty());

    let Some(endpoint) = endpoint else {
        return (None, None);
    };

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
            eprintln!(
                "telemetry: OTLP exporter init failed ({err}); continuing without OTel"
            );
            return (None, None);
        }
    };

    let resource = Resource::builder()
        .with_service_name(telemetry.service_name.clone())
        .with_attributes([
            KeyValue::new(semconv::SERVICE_NAMESPACE, tenant_id.as_str().to_string()),
            KeyValue::new(semconv::SERVICE_VERSION, env!("CARGO_PKG_VERSION")),
            KeyValue::new(
                semconv::SERVICE_INSTANCE_ID,
                uuid::Uuid::new_v4().to_string(),
            ),
        ])
        .build();

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
