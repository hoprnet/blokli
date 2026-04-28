# OTLP Configuration

This document describes how to configure OpenTelemetry (OTLP) export for `bloklid`.

## Configuration File

Add OTLP settings under `[telemetry]` in your `bloklid` config file.

| Key                                | Required | Description                                                                                                                                               | Example                 |
| ---------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| `telemetry.otlp_endpoint`          | No       | Enables OTLP export when set. Transport is inferred from the scheme: `grpc://`, `http://`, or `https://`. Leave unset or empty to disable OTLP export.    | `http://localhost:4318` |
| `telemetry.otlp_signals`           | No       | Comma-separated signals to export. Supported values: `metrics`, `traces`, `logs`. Defaults to `metrics`. Invalid values are ignored.                      | `metrics,traces,logs`   |
| `telemetry.metric_export_interval` | No       | Export cadence for OTLP metrics. Uses humantime syntax such as `500ms`, `15s`, or `1m`. Defaults to `60s`. Must be greater than `0` when OTLP is enabled. | `15s`                   |

## Environment Overrides

`bloklid` also accepts environment overrides. These override values from the config file when both are set.

| Variable                        | Config Key                         | Description                  | Example                 |
| ------------------------------- | ---------------------------------- | ---------------------------- | ----------------------- |
| `BLOKLI_OTLP_ENDPOINT`          | `telemetry.otlp_endpoint`          | OTLP collector endpoint      | `http://localhost:4318` |
| `BLOKLI_OTLP_SIGNALS`           | `telemetry.otlp_signals`           | Comma-separated OTLP signals | `metrics,traces,logs`   |
| `BLOKLI_METRIC_EXPORT_INTERVAL` | `telemetry.metric_export_interval` | OTLP metrics export interval | `10s`                   |

## Signal Behavior

- `metrics` enables periodic OTLP metric export.
- `traces` enables OTLP span export.
- `logs` enables OTLP log export.
- If `telemetry.otlp_signals` is omitted, `bloklid` exports `metrics`.
- If `telemetry.otlp_signals` is present but empty or contains only invalid values, `bloklid` falls back to `metrics` and logs a warning.

## Metric Interval

`telemetry.metric_export_interval` is a single interval applied to OTLP metric export. It does not support per-metric or per-prefix
overrides.

Examples:

- `500ms`
  - export OTLP metrics every 500ms
- `15s`
  - export OTLP metrics every 15 seconds
- `1m`
  - export OTLP metrics every 60 seconds

Current code behavior:

- the interval is only used by the OTLP metrics exporter at runtime
- config loading still requires the interval to be greater than `0` whenever `telemetry.otlp_endpoint` is configured

## Endpoint Notes

- `grpc://...` selects OTLP/gRPC. Internally, `bloklid` normalizes this to the `http://...` URI form expected by the Rust tonic client, but
  users should keep configuring `grpc://...`.
- `http://...` and `https://...` select OTLP/HTTP.
- For OTLP/HTTP, configure a collector base URL such as `http://localhost:4318`. `bloklid` appends `/v1/traces`, `/v1/logs`, or
  `/v1/metrics` internally per enabled signal.
- Legacy HTTP endpoints that already include `/v1/traces`, `/v1/logs`, or `/v1/metrics` are still accepted and normalized per signal.
- If you want to export `metrics`, `traces`, and `logs` together, OTLP/gRPC remains the cleaner single-endpoint option.

## Example Configurations

### Local HTTP OTLP Ingestor (metrics only)

```toml
[telemetry]
otlp_endpoint = "http://localhost:4318"
otlp_signals = "metrics"
metric_export_interval = "10s"
```

### Collector gRPC (all signals)

```toml
[telemetry]
otlp_endpoint = "grpc://otel-collector:4317"
otlp_signals = "metrics,traces,logs"
metric_export_interval = "15s"
```

### Environment Override Example

```bash
export BLOKLI_OTLP_ENDPOINT=http://localhost:4318
export BLOKLI_OTLP_SIGNALS=metrics,traces,logs
export BLOKLI_METRIC_EXPORT_INTERVAL=15s
bloklid -c bloklid/example-config.toml
```

## Metrics Endpoint

- OTLP export is additive. It does not replace the Prometheus-style `/metrics` endpoint.
- When the embedded API server is enabled and built with telemetry support, the service still exposes `blokli_*` metrics on `/metrics`.
