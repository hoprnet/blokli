# Blokli Helm Chart

[Blokli](https://github.com/hoprnet/blokli) is an on-chain indexer for HOPR smart contracts with a GraphQL API server for querying indexed blockchain data.

## TL;DR

```bash
helm install my-blokli ./charts/blokli \
  --set database.host=postgresql.default.svc.cluster.local \
  --set database.password=secretpassword \
  --set config.network=jura-prod \
  --set config.rpcUrl=https://rpc.gnosischain.com
```

## Introduction

This chart bootstraps a Blokli deployment on a Kubernetes cluster using the Helm package manager.

## Prerequisites

- Kubernetes 1.19+
- Helm 3.0+
- PersistentVolume provisioner support in the underlying infrastructure
- PostgreSQL database (external)

## Installing the Chart

To install the chart with the release name `my-blokli`:

```bash
helm install my-blokli ./charts/blokli -f my-values.yaml
```

The command deploys Blokli on the Kubernetes cluster with the configuration specified in `my-values.yaml`. The [Parameters](#parameters) section lists the parameters that can be configured during installation.

## Uninstalling the Chart

To uninstall the `my-blokli` deployment:

```bash
helm uninstall my-blokli
```

The command removes all the Kubernetes components associated with the chart and deletes the release.

## Parameters

### Global Configuration ###

| Name                                                                                   | Description                                                                                                                                                                                                                                                                                                | Value                                                       |
| -------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| `image.registry`                                                                       | Container image registry                                                                                                                                                                                                                                                                                   | `europe-west3-docker.pkg.dev/hoprassociation/docker-images` |
| `image.repository`                                                                     | Container image repository                                                                                                                                                                                                                                                                                 | `bloklid`                                                   |
| `image.pullPolicy`                                                                     | Container image pull policy                                                                                                                                                                                                                                                                                | `Always`                                                    |
| `image.tag`                                                                            | Container image tag (defaults to chart appVersion if not specified)                                                                                                                                                                                                                                        | `""`                                                        |
| `imagePullSecrets`                                                                     | Image pull secrets for private registries                                                                                                                                                                                                                                                                  | `[]`                                                        |
| `nameOverride`                                                                         | Override the chart name                                                                                                                                                                                                                                                                                    | `""`                                                        |
| `fullnameOverride`                                                                     | Override the full release name                                                                                                                                                                                                                                                                             | `""`                                                        |
| `serviceAccount.create`                                                                | Specifies whether a service account should be created                                                                                                                                                                                                                                                      | `true`                                                      |
| `serviceAccount.annotations`                                                           | Annotations to add to the service account                                                                                                                                                                                                                                                                  | `{}`                                                        |
| `serviceAccount.name`                                                                  | The name of the service account to use. If not set and create is true, a name is generated using the fullname template                                                                                                                                                                                     | `""`                                                        |
| `deploymentAnnotations`                                                                | Annotations to add to the deployment                                                                                                                                                                                                                                                                       | `{}`                                                        |
| `podAnnotations`                                                                       | Annotations to add to the pod                                                                                                                                                                                                                                                                              | `{}`                                                        |
| `podSecurityContext.runAsNonRoot`                                                      | Run container as non-root user                                                                                                                                                                                                                                                                             | `true`                                                      |
| `podSecurityContext.runAsUser`                                                         | User ID to run the container                                                                                                                                                                                                                                                                               | `65532`                                                     |
| `podSecurityContext.runAsGroup`                                                        | Group ID to run the container                                                                                                                                                                                                                                                                              | `65532`                                                     |
| `podSecurityContext.fsGroup`                                                           | File system group ID                                                                                                                                                                                                                                                                                       | `65532`                                                     |
| `securityContext.allowPrivilegeEscalation`                                             | Prevent privilege escalation                                                                                                                                                                                                                                                                               | `false`                                                     |
| `securityContext.capabilities.drop`                                                    | Capabilities to drop                                                                                                                                                                                                                                                                                       | `["ALL"]`                                                   |
| `securityContext.readOnlyRootFilesystem`                                               | Mount root filesystem as read-only                                                                                                                                                                                                                                                                         | `true`                                                      |
| `securityContext.runAsNonRoot`                                                         | Run container as non-root user                                                                                                                                                                                                                                                                             | `true`                                                      |
| `securityContext.runAsUser`                                                            | User ID to run the container                                                                                                                                                                                                                                                                               | `65532`                                                     |
| `service.type`                                                                         | Service type                                                                                                                                                                                                                                                                                               | `ClusterIP`                                                 |
| `service.port`                                                                         | Service port for GraphQL API                                                                                                                                                                                                                                                                               | `8080`                                                      |
| `service.annotations`                                                                  | Service annotations                                                                                                                                                                                                                                                                                        | `{}`                                                        |
| `ingress.enabled`                                                                      | Enable ingress for GraphQL API                                                                                                                                                                                                                                                                             | `false`                                                     |
| `ingress.className`                                                                    | Ingress class name                                                                                                                                                                                                                                                                                         | `""`                                                        |
| `ingress.annotations`                                                                  | Ingress annotations                                                                                                                                                                                                                                                                                        | `{}`                                                        |
| `ingress.hostname`                                                                     | Hostnames for the GraphQL API                                                                                                                                                                                                                                                                              | `""`                                                        |
| `ingress.tls`                                                                          | Enable TLS configuration for ingress. It will reference a secret named <hostname>-tls. Where hostname dots are replaced with dashes.                                                                                                                                                                       | `true`                                                      |
| `resources`                                                                            | Resource requests and limits                                                                                                                                                                                                                                                                               | `{}`                                                        |
| `livenessProbe.enabled`                                                                | Enable liveness probe                                                                                                                                                                                                                                                                                      | `true`                                                      |
| `livenessProbe.initialDelaySeconds`                                                    | Initial delay before liveness probe starts                                                                                                                                                                                                                                                                 | `30`                                                        |
| `livenessProbe.periodSeconds`                                                          | Period between liveness probe checks                                                                                                                                                                                                                                                                       | `10`                                                        |
| `livenessProbe.timeoutSeconds`                                                         | Timeout for liveness probe                                                                                                                                                                                                                                                                                 | `5`                                                         |
| `livenessProbe.successThreshold`                                                       | Success threshold for liveness probe                                                                                                                                                                                                                                                                       | `1`                                                         |
| `livenessProbe.failureThreshold`                                                       | Failure threshold for liveness probe                                                                                                                                                                                                                                                                       | `3`                                                         |
| `readinessProbe.enabled`                                                               | Enable readiness probe                                                                                                                                                                                                                                                                                     | `true`                                                      |
| `readinessProbe.initialDelaySeconds`                                                   | Initial delay before readiness probe starts                                                                                                                                                                                                                                                                | `10`                                                        |
| `readinessProbe.periodSeconds`                                                         | Period between readiness probe checks                                                                                                                                                                                                                                                                      | `5`                                                         |
| `readinessProbe.timeoutSeconds`                                                        | Timeout for readiness probe                                                                                                                                                                                                                                                                                | `3`                                                         |
| `readinessProbe.successThreshold`                                                      | Success threshold for readiness probe                                                                                                                                                                                                                                                                      | `1`                                                         |
| `readinessProbe.failureThreshold`                                                      | Failure threshold for readiness probe                                                                                                                                                                                                                                                                      | `3`                                                         |
| `startupProbe.enabled`                                                                 | Enable startup probe                                                                                                                                                                                                                                                                                       | `true`                                                      |
| `startupProbe.initialDelaySeconds`                                                     | Initial delay before startup probe starts                                                                                                                                                                                                                                                                  | `0`                                                         |
| `startupProbe.periodSeconds`                                                           | Period between startup probe checks                                                                                                                                                                                                                                                                        | `10`                                                        |
| `startupProbe.timeoutSeconds`                                                          | Timeout for startup probe                                                                                                                                                                                                                                                                                  | `3`                                                         |
| `startupProbe.successThreshold`                                                        | Success threshold for startup probe                                                                                                                                                                                                                                                                        | `1`                                                         |
| `startupProbe.failureThreshold`                                                        | Failure threshold for startup probe (allows up to 5 minutes for startup)                                                                                                                                                                                                                                   | `30`                                                        |
| `nodeSelector`                                                                         | Node selector for pod assignment                                                                                                                                                                                                                                                                           | `{}`                                                        |
| `tolerations`                                                                          | Tolerations for pod assignment                                                                                                                                                                                                                                                                             | `[]`                                                        |
| `affinity`                                                                             | Affinity rules for pod assignment                                                                                                                                                                                                                                                                          | `{}`                                                        |
| `monitoring.serviceMonitor.enabled`                                                    | Enable ServiceMonitor creation                                                                                                                                                                                                                                                                             | `false`                                                     |
| `monitoring.serviceMonitor.namespace`                                                  | ServiceMonitor namespace (defaults to release namespace)                                                                                                                                                                                                                                                   | `""`                                                        |
| `monitoring.serviceMonitor.labels`                                                     | ServiceMonitor labels                                                                                                                                                                                                                                                                                      | `{}`                                                        |
| `monitoring.serviceMonitor.interval`                                                   | Scrape interval                                                                                                                                                                                                                                                                                            | `30s`                                                       |
| `monitoring.serviceMonitor.scrapeTimeout`                                              | Scrape timeout                                                                                                                                                                                                                                                                                             | `10s`                                                       |
| `monitoring.serviceMonitor.path`                                                       | Metrics path exposed by the embedded API server                                                                                                                                                                                                                                                            | `/metrics`                                                  |
| `monitoring.prometheusRule.enabled`                                                    | Enable PrometheusRule (alerting rules) creation. Requires the Prometheus Operator CRDs                                                                                                                                                                                                                     | `false`                                                     |
| `monitoring.prometheusRule.namespace`                                                  | PrometheusRule namespace (defaults to release namespace)                                                                                                                                                                                                                                                   | `""`                                                        |
| `monitoring.prometheusRule.labels`                                                     | Additional labels on the PrometheusRule resource (e.g. to match a Prometheus Operator ruleSelector)                                                                                                                                                                                                        | `{}`                                                        |
| `monitoring.prometheusRule.rules.indexerLagHigh.enabled`                               | Enable the blokli-indexer-lag-high alert                                                                                                                                                                                                                                                                   | `true`                                                      |
| `monitoring.prometheusRule.rules.indexerLagHigh.thresholdBlocks`                       | Number of blocks the indexer may lag behind the chain head before firing                                                                                                                                                                                                                                   | `15`                                                        |
| `monitoring.prometheusRule.rules.indexerLagHigh.for`                                   | How long the lag must stay above the threshold before firing                                                                                                                                                                                                                                               | `5m`                                                        |
| `monitoring.prometheusRule.rules.indexerStalled.enabled`                               | Enable the blokli-indexer-stalled alert                                                                                                                                                                                                                                                                    | `true`                                                      |
| `monitoring.prometheusRule.rules.indexerStalled.windowMinutes`                         | Look-back window (minutes) used to detect indexer progress against chain head progress                                                                                                                                                                                                                     | `5`                                                         |
| `monitoring.prometheusRule.rules.indexerStalled.for`                                   | How long the stalled condition must persist before firing                                                                                                                                                                                                                                                  | `2m`                                                        |
| `monitoring.prometheusRule.rules.unhealthy.enabled`                                    | Enable the blokli-unhealthy alert                                                                                                                                                                                                                                                                          | `true`                                                      |
| `monitoring.prometheusRule.rules.unhealthy.for`                                        | How long readiness must report unhealthy before firing                                                                                                                                                                                                                                                     | `5m`                                                        |
| `monitoring.prometheusRule.rules.stateCorrupted.enabled`                               | Enable the blokli-state-corrupted alert                                                                                                                                                                                                                                                                    | `true`                                                      |
| `monitoring.prometheusRule.rules.stateCorrupted.for`                                   | How long the corrupted status must persist before firing                                                                                                                                                                                                                                                   | `1m`                                                        |
| `monitoring.prometheusRule.rules.transactionFailed.enabled`                            | Enable the blokli-transaction-failed alert                                                                                                                                                                                                                                                                 | `true`                                                      |
| `monitoring.prometheusRule.rules.transactionFailed.windowMinutes`                      | Short look-back window (minutes) for the recent failure rate                                                                                                                                                                                                                                               | `15`                                                        |
| `monitoring.prometheusRule.rules.transactionFailed.minFailures`                        | Minimum absolute number of failures in windowMinutes before this counts as a spike at all, regardless of ratio                                                                                                                                                                                             | `3`                                                         |
| `monitoring.prometheusRule.rules.transactionFailed.baselineWindowHours`                | Longer look-back window (hours) establishing the "normal" recent failure rate to compare against                                                                                                                                                                                                           | `6`                                                         |
| `monitoring.prometheusRule.rules.transactionFailed.spikeMultiplier`                    | How many times higher than the baseline rate the recent rate must be to count as a spike                                                                                                                                                                                                                   | `3`                                                         |
| `monitoring.prometheusRule.rules.transactionFailed.for`                                | How long the spike must persist across evaluations before firing                                                                                                                                                                                                                                           | `1m`                                                        |
| `monitoring.prometheusRule.rules.transactionValidationFailed.enabled`                  | Enable the blokli-transaction-validation-failed alert                                                                                                                                                                                                                                                      | `true`                                                      |
| `monitoring.prometheusRule.rules.transactionValidationFailed.windowMinutes`            | Look-back window (minutes) used to detect transactions rejected before submission (malformed/disallowed raw transactions)                                                                                                                                                                                  | `15`                                                        |
| `monitoring.prometheusRule.rules.transactionValidationFailed.minFailures`              | Minimum number of validation failures in windowMinutes before firing, so a handful of one-off bad requests (a stale client, a few scanner probes) doesn't page on its own                                                                                                                                  | `5`                                                         |
| `monitoring.prometheusRule.rules.transactionValidationFailed.for`                      | How long the failure must persist across evaluations before firing                                                                                                                                                                                                                                         | `5m`                                                        |
| `monitoring.prometheusRule.rules.rpcFailureRateHigh.enabled`                           | Enable the blokli-rpc-failure-rate-high alert                                                                                                                                                                                                                                                              | `true`                                                      |
| `monitoring.prometheusRule.rules.rpcFailureRateHigh.thresholdRatio`                    | Fraction (0-1) of failed RPC calls over the evaluation window before firing                                                                                                                                                                                                                                | `0.1`                                                       |
| `monitoring.prometheusRule.rules.rpcFailureRateHigh.for`                               | How long the failure ratio must stay above the threshold before firing                                                                                                                                                                                                                                     | `10m`                                                       |
| `monitoring.prometheusRule.rules.rpcCallLatencyHigh.enabled`                           | Enable the blokli-rpc-call-latency-high alert                                                                                                                                                                                                                                                              | `true`                                                      |
| `monitoring.prometheusRule.rules.rpcCallLatencyHigh.thresholdSeconds`                  | p95 RPC call latency (seconds) before firing                                                                                                                                                                                                                                                               | `5`                                                         |
| `monitoring.prometheusRule.rules.rpcCallLatencyHigh.for`                               | How long the p95 latency must stay above the threshold before firing                                                                                                                                                                                                                                       | `10m`                                                       |
| `monitoring.prometheusRule.rules.rpcRetriesHigh.enabled`                               | Enable the blokli-rpc-retries-high alert. Defaults to disabled: blokli_retries_per_rpc_call has no code path that ever records it yet (blocked on TODO(#7140) in chain/rpc/src/client.rs), so this alert cannot fire regardless of the threshold until that lands. See runbooks/blokli-rpc-retries-high.md | `false`                                                     |
| `monitoring.prometheusRule.rules.rpcRetriesHigh.thresholdRetries`                      | p95 retries per RPC call before firing                                                                                                                                                                                                                                                                     | `3`                                                         |
| `monitoring.prometheusRule.rules.rpcRetriesHigh.for`                                   | How long the p95 retries must stay above the threshold before firing                                                                                                                                                                                                                                       | `10m`                                                       |
| `monitoring.prometheusRule.rules.graphqlErrorRateHigh.enabled`                         | Enable the blokli-graphql-error-rate-high alert                                                                                                                                                                                                                                                            | `true`                                                      |
| `monitoring.prometheusRule.rules.graphqlErrorRateHigh.thresholdRatio`                  | Fraction (0-1) of query/mutation requests resulting in errors over the evaluation window before firing. Does not apply to invalid_request or subscription, which have no meaningful request-count denominator (see thresholdInvalidRequestCount / thresholdSubscriptionErrorCount)                         | `0.05`                                                      |
| `monitoring.prometheusRule.rules.graphqlErrorRateHigh.thresholdInvalidRequestCount`    | Number of malformed/unparseable GraphQL requests over a 5m window before firing                                                                                                                                                                                                                            | `5`                                                         |
| `monitoring.prometheusRule.rules.graphqlErrorRateHigh.thresholdSubscriptionErrorCount` | Number of errored messages pushed over GraphQL subscription connections over a 5m window before firing                                                                                                                                                                                                     | `5`                                                         |
| `monitoring.prometheusRule.rules.graphqlErrorRateHigh.for`                             | How long any one of the three conditions above must stay breached before firing                                                                                                                                                                                                                            | `10m`                                                       |
| `monitoring.prometheusRule.rules.graphqlLatencyHigh.enabled`                           | Enable the blokli-graphql-latency-high alert                                                                                                                                                                                                                                                               | `true`                                                      |
| `monitoring.prometheusRule.rules.graphqlLatencyHigh.thresholdSeconds`                  | p95 GraphQL request latency (seconds) before firing                                                                                                                                                                                                                                                        | `2`                                                         |
| `monitoring.prometheusRule.rules.graphqlLatencyHigh.for`                               | How long the p95 latency must stay above the threshold before firing                                                                                                                                                                                                                                       | `10m`                                                       |
| `extraEnv`                                                                             | Additional environment variables                                                                                                                                                                                                                                                                           | `[]`                                                        |
| `extraVolumes`                                                                         | Additional volumes                                                                                                                                                                                                                                                                                         | `[]`                                                        |
| `extraVolumeMounts`                                                                    | Additional volume mounts                                                                                                                                                                                                                                                                                   | `[]`                                                        |

### Blokli Application Configuration ###

| Name                                                 | Description                                                                                                   | Value               |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- | ------------------- |
| `persistence.enabled`                                | Enable persistence for data directory                                                                         | `true`              |
| `persistence.storageClass`                           | Storage class for PVC                                                                                         | `""`                |
| `persistence.accessModes`                            | Access mode for PVC                                                                                           | `["ReadWriteOnce"]` |
| `persistence.size`                                   | Size of the PVC                                                                                               | `10Gi`              |
| `persistence.annotations`                            | Annotations for the PVC                                                                                       | `{}`                |
| `persistence.selector`                               | Selector for existing PV (optional)                                                                           | `{}`                |
| `database.type`                                      | Database type (e.g., postgresql, sqlite)                                                                      | `""`                |
| `database.host`                                      | PostgreSQL host                                                                                               | `""`                |
| `database.port`                                      | PostgreSQL port                                                                                               | `5432`              |
| `database.database`                                  | PostgreSQL database name                                                                                      | `""`                |
| `database.username`                                  | PostgreSQL username                                                                                           | `""`                |
| `database.password`                                  | PostgreSQL password (ignored if existingSecret is set)                                                        | `""`                |
| `database.index_path`                                | Sqlite database file path (used only if database.type is "sqlite")                                            | `""`                |
| `database.logs_path`                                 | Sqlite logs database file path (used only if database.type is "sqlite")                                       | `""`                |
| `database.existingSecret`                            | Name of existing secret containing database credentials                                                       | `""`                |
| `database.maxConnections`                            | Maximum number of database connections                                                                        | `10`                |
| `config.network`                                     | HOPR network to index (anvil-localhost, jura-dev, jura-prod, piz-palu-dev, piz-palu-staging)                  | `jura-prod`         |
| `config.rpcUrl`                                      | Blockchain RPC URL                                                                                            | `""`                |
| `config.maxRpcRequestsPerSec`                        | Maximum RPC requests per second (0 = unlimited)                                                               | `0`                 |
| `config.dataDirectory`                               | Data directory path (should match persistence mount path)                                                     | `/data`             |
| `config.indexer.fastSync`                            | Enable fast sync mode                                                                                         | `true`              |
| `config.indexer.enableLogsSnapshot`                  | Enable logs snapshot feature                                                                                  | `false`             |
| `config.indexer.enableSafeIndexing`                  | Enable Safe contract event indexing for discovered Safes                                                      | `false`             |
| `config.indexer.logsSnapshotUrl`                     | URL for logs snapshot download (required when enableLogsSnapshot is true)                                     | `""`                |
| `config.indexer.subscription.eventBusCapacity`       | Capacity of the event bus buffer for channel events                                                           | `1000`              |
| `config.indexer.subscription.shutdownSignalCapacity` | Capacity of the shutdown signal buffer                                                                        | `10`                |
| `config.indexer.subscription.batchSize`              | Batch size for Phase 1 historical channel queries                                                             | `100`               |
| `config.api.enabled`                                 | Enable or disable the GraphQL API server                                                                      | `true`              |
| `config.api.bindAddress`                             | Address and port for the API server to bind to                                                                | `0.0.0.0:8080`      |
| `config.api.playgroundEnabled`                       | Enable GraphQL Playground for development and testing. Recommended to set to false for production deployments | `false`             |
| `config.api.gasMultiplier`                           | Multiplier for gas estimation to provide a safety margin (e.g., 1.2 for 20% extra)                            | `1.2`               |
| `config.api.health.maxIndexerLag`                    | Maximum indexer lag (blocks) before readiness fails                                                           | `10`                |
| `config.api.health.timeout`                          | Timeout for health check operations                                                                           | `5s`                |
| `config.api.health.readinessCheckInterval`           | Interval for periodic readiness checks                                                                        | `60s`               |
| `config.logging.level`                               | Rust log level configuration. Examples: "info", "debug", "info,blokli_chain_indexer=debug"                    | `info`              |
| `config.logging.backtrace`                           | Rust backtrace configuration (e.g., "full", "short", "0")                                                     | `full`              |
| `config.logging.format`                              | Log output format ("json" or empty for plain text)                                                            | `json`              |
| `config.telemetry.otlpEndpoint`                      | OTLP endpoint for telemetry data                                                                              | `""`                |
| `config.telemetry.otlpSignals`                       | OTLP signals to export (e.g., "metrics", "traces")                                                            | `metrics`           |
| `config.telemetry.metricExportInterval`              | Interval for exporting metrics                                                                                | `15s`               |

## Configuration Examples

### Full example Configuration

Please refer to the contents of the file [values-testing.yaml](./values-testing.yaml) for a full example configuration

### Configuration with Existing Secret

```yaml
# values-secret.yaml
database:
  host: postgresql.database.svc.cluster.local
  existingSecret: blokli-db-credentials
  # The secret must contain keys: DB_USERNAME and DB_PASSWORD
```

Create the secret:

```bash
kubectl create secret generic blokli-db-credentials \
  --from-literal=DB_USERNAME=bloklid \
  --from-literal=DB_PASSWORD=my-secure-password
```

## Deployment Architectures

### Unified Deployment (Default)

This chart deploys bloklid with an embedded GraphQL API server. This is the simplest architecture and suitable for most deployments.

```
┌─────────────────┐
│   bloklid Pod   │
│  (Indexer +     │
│   GraphQL API)  │
└────────┬────────┘
         │
    ┌────▼─────┐
    │ Service  │
    └──────────┘
```

## Prerequisites Setup

### PostgreSQL Database

Blokli requires an external PostgreSQL database. You can use a managed service (CloudSQL, RDS, Azure Database) or deploy PostgreSQL in your cluster.

Example using Bitnami PostgreSQL chart:

```bash
helm repo add bitnamilelegacy https://raw.githubusercontent.com/bitnami/charts/archive-full-index/bitnami
helm install postgresql bitnamilelegacy/postgresql \
  --set auth.username=bloklid \
  --set auth.password=secretpassword \
  --set auth.database=bloklid
```

Initialize the database (bloklid runs migrations automatically on startup).

## Accessing the GraphQL API

### Via Port Forwarding

```bash
kubectl port-forward svc/my-blokli 8080:8080
# Access at http://localhost:8080/graphql
```

### Via Ingress

If ingress is enabled, access via the configured hostname:

```bash
curl https://blokli.example.com/graphql
```

### GraphQL Endpoints

- **GraphQL API**: `/graphql`
- **GraphQL Subscriptions (SSE)**: `/graphql/subscriptions`
- **Health Check (liveness)**: `/healthz`
- **Health Check (readiness)**: `/readyz`
- **GraphQL Playground**: `/graphql` (in browser)

## Upgrading

To upgrade the chart:

```bash
helm upgrade my-blokli ./charts/blokli -f my-values.yaml
```

## Troubleshooting

### Check Pod Status

```bash
kubectl get pods -l app.kubernetes.io/name=blokli
kubectl describe pod <pod-name>
kubectl logs <pod-name>
```

### Check Database Connectivity

```bash
kubectl exec -it <pod-name> -- sh
# Then test database connection
```

### Check Configuration

```bash
kubectl get configmap my-blokli -o yaml
```

### Common Issues

1. **Pod not starting**: Check database connectivity and credentials
2. **OOM errors**: Increase memory limits in resources
3. **Slow indexing**: Increase RPC rate limit or check RPC provider
4. **Disk full**: Increase persistence size

## License

GPL-3.0-or-later

## Support

For issues and feature requests, please visit:

- GitHub Issues: <https://github.com/hoprnet/blokli/issues>
- Documentation: <https://github.com/hoprnet/blokli>
