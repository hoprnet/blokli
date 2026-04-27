use std::{
    collections::HashMap,
    str::FromStr,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use flagset::FlagSet;
use opentelemetry::{
    Key, KeyValue,
    logs::{AnyValue, LogRecord as _, Logger as _, LoggerProvider as _, Severity},
    trace::TracerProvider as _,
};
use opentelemetry_otlp::{Protocol, WithExportConfig as _};
use opentelemetry_sdk::{
    Resource,
    logs::{SdkLogger, SdkLoggerProvider, log_processor_with_async_runtime::BatchLogProcessor},
    metrics::{SdkMeterProvider, periodic_reader_with_async_runtime::PeriodicReader},
    runtime::Tokio,
    trace::{RandomIdGenerator, Sampler, SdkTracerProvider, span_processor_with_async_runtime::BatchSpanProcessor},
};
use strum::{Display, EnumString};
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use url::Url;

use crate::{
    config::TelemetryConfig,
    errors::{BloklidError, Result},
    telemetry_common::{self, OtelBoxedLayer},
};

#[derive(Default)]
pub struct TelemetryHandles {
    tracer_provider: Option<SdkTracerProvider>,
    logger_provider: Option<SdkLoggerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl Drop for TelemetryHandles {
    fn drop(&mut self) {
        if let Some(tracer_provider) = self.tracer_provider.take() {
            let _ = tracer_provider.shutdown();
        }
        if let Some(logger_provider) = self.logger_provider.take() {
            let _ = logger_provider.shutdown();
        }
        if let Some(meter_provider) = self.meter_provider.take() {
            let _ = meter_provider.shutdown();
        }
    }
}

flagset::flags! {
    #[repr(u8)]
    #[derive(PartialOrd, Ord, EnumString, Display)]
    pub(crate) enum OtlpSignal: u8 {
        #[strum(serialize = "traces")]
        Traces = 0b0000_0001,

        #[strum(serialize = "logs")]
        Logs = 0b0000_0010,

        #[strum(serialize = "metrics")]
        Metrics = 0b0000_0100,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, Display)]
enum OtlpTransport {
    #[strum(serialize = "grpc")]
    Grpc,

    #[strum(serialize = "http", serialize = "https")]
    Http,
}

#[derive(Debug)]
struct OtlpConfig {
    endpoint: String,
    transport: OtlpTransport,
    signals: FlagSet<OtlpSignal>,
    metric_export_interval: Duration,
}

impl OtlpConfig {
    fn from_config(config: &TelemetryConfig) -> Result<Option<Self>> {
        let Some(endpoint) = config
            .otlp_endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };

        let transport = infer_otlp_transport(endpoint)?;
        Ok(Some(Self {
            endpoint: normalize_otlp_endpoint(endpoint, transport),
            transport,
            signals: parse_otlp_signals(&config.otlp_signals),
            metric_export_interval: config.metric_export_interval,
        }))
    }

    fn has_signal(&self, signal: OtlpSignal) -> bool {
        self.signals.contains(signal)
    }

    fn endpoint_for(&self, signal: OtlpSignal) -> Result<String> {
        resolve_otlp_endpoint(&self.endpoint, self.transport, signal)
    }
}

#[derive(Clone)]
struct OtelLogsLayer {
    logger: SdkLogger,
}

impl OtelLogsLayer {
    fn new(logger: SdkLogger) -> Self {
        Self { logger }
    }
}

impl<S> tracing_subscriber::Layer<S> for OtelLogsLayer
where
    S: tracing::Subscriber,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        let metadata = event.metadata();
        let mut visitor = TracingEventVisitor::default();
        event.record(&mut visitor);

        let mut record = self.logger.create_log_record();
        let event_timestamp = visitor.timestamp.unwrap_or(SystemTime::now());

        let (severity_number, severity_text) = match *metadata.level() {
            tracing::Level::ERROR => (Severity::Error, "ERROR"),
            tracing::Level::WARN => (Severity::Warn, "WARN"),
            tracing::Level::INFO => (Severity::Info, "INFO"),
            tracing::Level::DEBUG => (Severity::Debug, "DEBUG"),
            tracing::Level::TRACE => (Severity::Trace, "TRACE"),
        };

        record.set_timestamp(event_timestamp);
        record.set_observed_timestamp(event_timestamp);
        record.set_target(metadata.target().to_string());
        record.set_severity_number(severity_number);
        record.set_severity_text(severity_text);

        if let Some(message) = visitor.body.take() {
            let body = HashMap::from([(Key::new("message"), AnyValue::String(message.into()))]);
            record.set_body(AnyValue::Map(Box::new(body)));
        }
        if let Some(module_path) = metadata.module_path() {
            record.add_attribute("module_path", module_path.to_string());
        }
        if let Some(file) = metadata.file() {
            record.add_attribute("file", file.to_string());
        }
        if let Some(line) = metadata.line() {
            record.add_attribute("line", i64::from(line));
        }
        record.add_attribute("target", metadata.target().to_string());
        if !visitor.attributes.is_empty() {
            record.add_attributes(visitor.attributes);
        }

        self.logger.emit(record);
    }
}

#[derive(Default)]
struct TracingEventVisitor {
    body: Option<String>,
    attributes: Vec<(String, AnyValue)>,
    timestamp: Option<std::time::SystemTime>,
}

impl TracingEventVisitor {
    fn record_body_or_attribute<V>(&mut self, field: &Field, value: V)
    where
        V: Into<AnyValue> + ToString,
    {
        if field.name() == "message" {
            self.body = Some(value.to_string());
        } else {
            self.attributes.push((field.name().to_string(), value.into()));
        }
    }

    fn maybe_record_unix_timestamp_millis(&mut self, field: &Field, value: u64) {
        if field.name() == "timestamp" && self.timestamp.is_none() {
            self.timestamp = UNIX_EPOCH.checked_add(Duration::from_millis(value));
        }
    }
}

impl Visit for TracingEventVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if let Ok(value) = u64::try_from(value) {
            self.maybe_record_unix_timestamp_millis(field, value);
        }
        self.record_body_or_attribute(field, value);
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.maybe_record_unix_timestamp_millis(field, value);
        if value <= i64::MAX as u64 {
            self.record_body_or_attribute(field, value as i64);
        } else {
            self.record_body_or_attribute(field, value.to_string());
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_body_or_attribute(field, value);
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_body_or_attribute(field, value.to_string());
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.record_body_or_attribute(field, value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_body_or_attribute(field, format!("{value:?}"));
    }
}

pub fn init(verbosity: u8, config: &TelemetryConfig) -> Result<TelemetryHandles> {
    let _ = verbosity;
    let otlp_config = OtlpConfig::from_config(config)?;

    let resource = Resource::builder()
        .with_service_name(env!("CARGO_PKG_NAME").to_string())
        .with_attributes([
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            KeyValue::new("telemetry.sdk.language", "rust"),
        ])
        .build();

    let mut handles = TelemetryHandles::default();
    let mut otel_layers: Vec<OtelBoxedLayer> = Vec::new();

    let trace_layer = if let Some(otlp_config) = otlp_config
        .as_ref()
        .filter(|config| config.has_signal(OtlpSignal::Traces))
    {
        let exporter = match otlp_config.transport {
            OtlpTransport::Grpc => opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_protocol(Protocol::Grpc)
                .with_endpoint(otlp_config.endpoint_for(OtlpSignal::Traces)?)
                .with_timeout(Duration::from_secs(5))
                .build(),
            OtlpTransport::Http => opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(otlp_config.endpoint_for(OtlpSignal::Traces)?)
                .with_timeout(Duration::from_secs(5))
                .build(),
        }
        .map_err(|error| BloklidError::NonSpecific(format!("failed to build OTLP trace exporter: {error}")))?;

        let batch_processor = BatchSpanProcessor::builder(exporter, Tokio).build();
        let tracer_provider = SdkTracerProvider::builder()
            .with_span_processor(batch_processor)
            .with_sampler(Sampler::AlwaysOn)
            .with_id_generator(RandomIdGenerator::default())
            .with_max_events_per_span(64)
            .with_max_attributes_per_span(16)
            .with_resource(resource.clone())
            .build();
        let tracer = tracer_provider.tracer(env!("CARGO_PKG_NAME"));
        handles.tracer_provider = Some(tracer_provider);
        Some(tracing_opentelemetry::layer().with_tracer(tracer).boxed())
    } else {
        None
    };

    let logs_layer = if let Some(otlp_config) = otlp_config
        .as_ref()
        .filter(|config| config.has_signal(OtlpSignal::Logs))
    {
        let exporter = match otlp_config.transport {
            OtlpTransport::Grpc => opentelemetry_otlp::LogExporter::builder()
                .with_tonic()
                .with_protocol(Protocol::Grpc)
                .with_endpoint(otlp_config.endpoint_for(OtlpSignal::Logs)?)
                .with_timeout(Duration::from_secs(5))
                .build(),
            OtlpTransport::Http => opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .with_protocol(opentelemetry_otlp::Protocol::HttpJson)
                .with_endpoint(otlp_config.endpoint_for(OtlpSignal::Logs)?)
                .with_timeout(Duration::from_secs(5))
                .build(),
        }
        .map_err(|error| BloklidError::NonSpecific(format!("failed to build OTLP log exporter: {error}")))?;

        let batch_processor = BatchLogProcessor::builder(exporter, Tokio).build();
        let logger_provider = SdkLoggerProvider::builder()
            .with_log_processor(batch_processor)
            .with_resource(resource.clone())
            .build();
        let logger = logger_provider.logger(env!("CARGO_PKG_NAME"));
        handles.logger_provider = Some(logger_provider);
        Some(Box::new(OtelLogsLayer::new(logger)) as OtelBoxedLayer)
    } else {
        None
    };

    let prometheus_exporter = opentelemetry_prometheus_text_exporter::PrometheusExporter::builder()
        .without_counter_suffixes()
        .without_units()
        .without_target_info()
        .without_scope_info()
        .build();

    let meter_provider = if let Some(otlp_config) = otlp_config
        .as_ref()
        .filter(|config| config.has_signal(OtlpSignal::Metrics))
    {
        let exporter = match otlp_config.transport {
            OtlpTransport::Grpc => opentelemetry_otlp::MetricExporter::builder()
                .with_tonic()
                .with_protocol(Protocol::Grpc)
                .with_endpoint(otlp_config.endpoint_for(OtlpSignal::Metrics)?)
                .with_timeout(Duration::from_secs(5))
                .build(),
            OtlpTransport::Http => opentelemetry_otlp::MetricExporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(otlp_config.endpoint_for(OtlpSignal::Metrics)?)
                .with_timeout(Duration::from_secs(5))
                .build(),
        }
        .map_err(|error| BloklidError::NonSpecific(format!("failed to build OTLP metric exporter: {error}")))?;

        let reader = PeriodicReader::builder(exporter, Tokio)
            .with_interval(otlp_config.metric_export_interval)
            .build();

        SdkMeterProvider::builder()
            .with_reader(reader)
            .with_reader(prometheus_exporter.clone())
            .with_resource(resource.clone())
            .build()
    } else {
        SdkMeterProvider::builder()
            .with_reader(prometheus_exporter.clone())
            .with_resource(resource.clone())
            .build()
    };

    opentelemetry::global::set_meter_provider(meter_provider.clone());

    if !hopr_metrics::init_with_provider(prometheus_exporter, meter_provider.clone()) {
        tracing::warn!("hopr-metrics global state was already initialized; custom provider not applied");
    }
    handles.meter_provider = Some(meter_provider);

    if let Some(trace_layer) = trace_layer {
        otel_layers.push(trace_layer);
    }
    if let Some(logs_layer) = logs_layer {
        otel_layers.push(logs_layer);
    }
    telemetry_common::install_otel_layers(otel_layers)?;

    if let Some(otlp_config) = otlp_config {
        tracing::info!(
            otlp_endpoint = otlp_config.endpoint,
            otlp_protocol = %otlp_config.transport,
            otlp_signals = %format_otlp_signals(otlp_config.signals),
            metric_export_interval_ms = otlp_config.metric_export_interval.as_millis() as u64,
            "OpenTelemetry export enabled"
        );
    }

    Ok(handles)
}

fn default_otlp_signals() -> FlagSet<OtlpSignal> {
    let mut signals = FlagSet::default();
    signals |= OtlpSignal::Metrics;
    signals
}

pub(crate) fn parse_otlp_signals(raw: &str) -> FlagSet<OtlpSignal> {
    let mut signals = FlagSet::default();

    for signal in raw.split(',') {
        let signal = signal.trim();
        if signal.is_empty() {
            continue;
        }

        match OtlpSignal::from_str(signal) {
            Ok(parsed) => signals |= parsed,
            Err(_) => tracing::warn!(otlp_signal = signal, "invalid telemetry.otlp_signals value ignored"),
        }
    }

    if signals.is_empty() {
        let fallback = default_otlp_signals();
        tracing::warn!(
            otlp_signals = raw,
            fallback_signals = %format_otlp_signals(fallback),
            "telemetry.otlp_signals yielded no valid signals; falling back to defaults"
        );
        return fallback;
    }

    signals
}

fn format_otlp_signals(signals: FlagSet<OtlpSignal>) -> String {
    [OtlpSignal::Traces, OtlpSignal::Logs, OtlpSignal::Metrics]
        .into_iter()
        .filter(|signal| signals.contains(*signal))
        .map(|signal| signal.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn infer_otlp_transport(endpoint: &str) -> Result<OtlpTransport> {
    let (scheme, _) = endpoint
        .split_once("://")
        .ok_or_else(|| BloklidError::NonSpecific("telemetry.otlp_endpoint must include a URL scheme".into()))?;

    OtlpTransport::from_str(scheme).map_err(|_| {
        BloklidError::NonSpecific(format!(
            "unsupported telemetry.otlp_endpoint scheme '{scheme}'; use grpc://, http://, or https://"
        ))
    })
}

fn normalize_otlp_endpoint(endpoint: &str, transport: OtlpTransport) -> String {
    if transport == OtlpTransport::Grpc {
        endpoint.replacen("grpc://", "http://", 1)
    } else {
        endpoint.to_string()
    }
}

fn resolve_otlp_endpoint(endpoint: &str, transport: OtlpTransport, signal: OtlpSignal) -> Result<String> {
    match transport {
        OtlpTransport::Grpc => Ok(normalize_otlp_endpoint(endpoint, transport)),
        OtlpTransport::Http => resolve_http_otlp_endpoint(endpoint, signal),
    }
}

fn resolve_http_otlp_endpoint(endpoint: &str, signal: OtlpSignal) -> Result<String> {
    let mut url = Url::parse(endpoint).map_err(|error| {
        BloklidError::NonSpecific(format!(
            "failed to parse telemetry.otlp_endpoint '{endpoint}' as an HTTP(S) URL: {error}"
        ))
    })?;
    let signal_path = otlp_http_signal_path(signal);
    let current_path = url.path();

    if current_path.is_empty() || current_path == "/" {
        url.set_path(signal_path);
        return Ok(url.to_string());
    }

    if let Some(prefix) = strip_known_http_signal_suffix(current_path) {
        let normalized_path = format!("{prefix}{signal_path}");
        url.set_path(&normalized_path);
        return Ok(url.to_string());
    }

    let normalized_path = format!("{}{signal_path}", current_path.trim_end_matches('/'));
    url.set_path(&normalized_path);
    Ok(url.to_string())
}

fn strip_known_http_signal_suffix(path: &str) -> Option<&str> {
    ["/v1/traces", "/v1/logs", "/v1/metrics"]
        .into_iter()
        .find_map(|suffix| path.strip_suffix(suffix))
}

fn otlp_http_signal_path(signal: OtlpSignal) -> &'static str {
    match signal {
        OtlpSignal::Traces => "/v1/traces",
        OtlpSignal::Logs => "/v1/logs",
        OtlpSignal::Metrics => "/v1/metrics",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_otlp_transport_from_http_endpoint() {
        assert_eq!(
            infer_otlp_transport("http://localhost:4318/v1/metrics").expect("http endpoint should parse"),
            OtlpTransport::Http
        );
        assert_eq!(
            infer_otlp_transport("https://collector.example/v1/metrics").expect("https endpoint should parse"),
            OtlpTransport::Http
        );
    }

    #[test]
    fn test_infer_otlp_transport_from_grpc_endpoint() {
        assert_eq!(
            infer_otlp_transport("grpc://localhost:4317").expect("grpc endpoint should parse"),
            OtlpTransport::Grpc
        );
    }

    #[test]
    fn test_infer_otlp_transport_rejects_invalid_scheme() {
        let error = infer_otlp_transport("ftp://localhost:4317").expect_err("unsupported scheme should fail");
        assert!(
            error.to_string().contains("unsupported telemetry.otlp_endpoint scheme"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn test_parse_otlp_signals_accepts_multiple_values() {
        let signals = parse_otlp_signals("metrics,traces,logs");
        assert!(signals.contains(OtlpSignal::Metrics));
        assert!(signals.contains(OtlpSignal::Traces));
        assert!(signals.contains(OtlpSignal::Logs));
    }

    #[test]
    fn test_parse_otlp_signals_ignores_invalid_value() {
        let signals = parse_otlp_signals("metrics,invalid");
        assert!(signals.contains(OtlpSignal::Metrics));
        assert!(!signals.contains(OtlpSignal::Traces));
        assert!(!signals.contains(OtlpSignal::Logs));
    }

    #[test]
    fn test_parse_otlp_signals_falls_back_for_empty_input() {
        let signals = parse_otlp_signals("");
        assert_eq!(signals, default_otlp_signals());
    }

    #[test]
    fn test_normalize_otlp_endpoint_for_grpc() {
        assert_eq!(
            normalize_otlp_endpoint("grpc://localhost:4317", OtlpTransport::Grpc),
            "http://localhost:4317"
        );
    }

    #[test]
    fn test_resolve_http_otlp_endpoint_appends_metric_path_for_base_url() {
        assert_eq!(
            resolve_http_otlp_endpoint("http://localhost:4318", OtlpSignal::Metrics)
                .expect("base collector URL should resolve"),
            "http://localhost:4318/v1/metrics"
        );
    }

    #[test]
    fn test_resolve_http_otlp_endpoint_appends_signal_path_for_nested_base_url() {
        assert_eq!(
            resolve_http_otlp_endpoint("https://collector.example/otel", OtlpSignal::Logs)
                .expect("nested base collector URL should resolve"),
            "https://collector.example/otel/v1/logs"
        );
    }

    #[test]
    fn test_resolve_http_otlp_endpoint_rewrites_existing_signal_suffix() {
        assert_eq!(
            resolve_http_otlp_endpoint("http://localhost:4318/v1/metrics", OtlpSignal::Traces)
                .expect("legacy full metrics path should be rewritten for traces"),
            "http://localhost:4318/v1/traces"
        );
    }

    #[test]
    fn test_resolve_otlp_endpoint_keeps_grpc_normalization() {
        assert_eq!(
            resolve_otlp_endpoint("grpc://localhost:4317", OtlpTransport::Grpc, OtlpSignal::Metrics)
                .expect("grpc endpoint should normalize"),
            "http://localhost:4317"
        );
    }
}
