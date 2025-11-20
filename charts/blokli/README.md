# Blokli Helm Chart

[Blokli](https://github.com/hoprnet/blokli) is an on-chain indexer for HOPR smart contracts with a GraphQL API server for querying indexed blockchain data.

## TL;DR

```bash
helm install my-blokli ./charts/blokli \
  --set image.repository=my-registry/bloklid \
  --set image.tag=1.0.0 \
  --set database.host=postgresql.default.svc.cluster.local \
  --set database.password=secretpassword \
  --set config.network=dufour \
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

### Global Parameters

| Name               | Description                                      | Value |
| ------------------ | ------------------------------------------------ | ----- |
| `replicaCount`     | Number of replicas (bloklid is singleton, use 1) | `1`   |
| `nameOverride`     | Override the chart name                          | `""`  |
| `fullnameOverride` | Override the full release name                   | `""`  |

### Image Parameters

| Name               | Description                                        | Value          |
| ------------------ | -------------------------------------------------- | -------------- |
| `image.registry`   | Container image registry                           | `""`           |
| `image.repository` | Container image repository                         | `""`           |
| `image.tag`        | Container image tag (defaults to chart appVersion) | `""`           |
| `image.pullPolicy` | Container image pull policy                        | `IfNotPresent` |
| `imagePullSecrets` | Image pull secrets for private registries          | `[]`           |

### Service Account Parameters

| Name                         | Description                 | Value  |
| ---------------------------- | --------------------------- | ------ |
| `serviceAccount.create`      | Create service account      | `true` |
| `serviceAccount.annotations` | Service account annotations | `{}`   |
| `serviceAccount.name`        | Service account name        | `""`   |

### Pod Parameters

| Name                 | Description                | Value           |
| -------------------- | -------------------------- | --------------- |
| `podAnnotations`     | Pod annotations            | `{}`            |
| `podSecurityContext` | Pod security context       | See values.yaml |
| `securityContext`    | Container security context | See values.yaml |

### Service Parameters

| Name                  | Description         | Value       |
| --------------------- | ------------------- | ----------- |
| `service.type`        | Service type        | `ClusterIP` |
| `service.port`        | Service port        | `3064`      |
| `service.annotations` | Service annotations | `{}`        |

### Ingress Parameters

| Name                  | Description                 | Value           |
| --------------------- | --------------------------- | --------------- |
| `ingress.enabled`     | Enable ingress              | `false`         |
| `ingress.className`   | Ingress class name          | `""`            |
| `ingress.annotations` | Ingress annotations         | `{}`            |
| `ingress.hosts`       | Ingress hosts configuration | See values.yaml |
| `ingress.tls`         | Ingress TLS configuration   | `[]`            |

### Database Parameters

| Name                      | Description                              | Value          |
| ------------------------- | ---------------------------------------- | -------------- |
| `database.host`           | PostgreSQL host                          | `"postgresql"` |
| `database.port`           | PostgreSQL port                          | `5432`         |
| `database.database`       | PostgreSQL database name                 | `"bloklid"`    |
| `database.username`       | PostgreSQL username                      | `"bloklid"`    |
| `database.password`       | PostgreSQL password                      | `""`           |
| `database.existingSecret` | Name of existing secret with credentials | `""`           |
| `database.maxConnections` | Maximum database connections             | `10`           |

### Blokli Configuration Parameters

| Name                                | Description                               | Value                       |
| ----------------------------------- | ----------------------------------------- | --------------------------- |
| `config.network`                    | HOPR network (dufour, rotsee, etc.)       | `"dufour"`                  |
| `config.rpcUrl`                     | Blockchain RPC URL                        | `"https://rpc.example.com"` |
| `config.maxRpcRequestsPerSec`       | Max RPC requests per second (0=unlimited) | `0`                         |
| `config.dataDirectory`              | Data directory path                       | `"/data"`                   |
| `config.indexer.fastSync`           | Enable fast sync mode                     | `true`                      |
| `config.indexer.enableLogsSnapshot` | Enable logs snapshot                      | `false`                     |
| `config.indexer.logsSnapshotUrl`    | Logs snapshot URL                         | `""`                        |
| `config.logging.level`              | Rust log level                            | `"info"`                    |

### Persistence Parameters

| Name                       | Description        | Value               |
| -------------------------- | ------------------ | ------------------- |
| `persistence.enabled`      | Enable persistence | `true`              |
| `persistence.storageClass` | Storage class      | `""`                |
| `persistence.accessModes`  | Access modes       | `["ReadWriteOnce"]` |
| `persistence.size`         | PVC size           | `10Gi`              |
| `persistence.annotations`  | PVC annotations    | `{}`                |

### Resource Parameters

| Name                        | Description    | Value   |
| --------------------------- | -------------- | ------- |
| `resources.limits.cpu`      | CPU limit      | `2000m` |
| `resources.limits.memory`   | Memory limit   | `4Gi`   |
| `resources.requests.cpu`    | CPU request    | `500m`  |
| `resources.requests.memory` | Memory request | `1Gi`   |

### Probe Parameters

| Name                     | Description            | Value  |
| ------------------------ | ---------------------- | ------ |
| `livenessProbe.enabled`  | Enable liveness probe  | `true` |
| `readinessProbe.enabled` | Enable readiness probe | `true` |
| `startupProbe.enabled`   | Enable startup probe   | `true` |

### Monitoring Parameters

| Name                                 | Description                      | Value      |
| ------------------------------------ | -------------------------------- | ---------- |
| `monitoring.serviceMonitor.enabled`  | Enable Prometheus ServiceMonitor | `false`    |
| `monitoring.serviceMonitor.interval` | Scrape interval                  | `30s`      |
| `monitoring.serviceMonitor.path`     | Metrics path                     | `/metrics` |

### Pod Disruption Budget Parameters

| Name                               | Description                | Value   |
| ---------------------------------- | -------------------------- | ------- |
| `podDisruptionBudget.enabled`      | Enable PodDisruptionBudget | `false` |
| `podDisruptionBudget.minAvailable` | Minimum available pods     | `1`     |

## Configuration Examples

### Basic Configuration with External PostgreSQL

```yaml
# values-production.yaml
image:
  repository: us-docker.pkg.dev/my-project/blokli/bloklid
  tag: "1.0.0"
  pullPolicy: IfNotPresent

database:
  host: postgresql.database.svc.cluster.local
  port: 5432
  database: bloklid
  username: bloklid
  password: "my-secure-password"

config:
  network: dufour
  rpcUrl: "https://rpc.gnosischain.com"
  maxRpcRequestsPerSec: 100

persistence:
  enabled: true
  storageClass: ssd
  size: 50Gi

resources:
  limits:
    cpu: 4000m
    memory: 8Gi
  requests:
    cpu: 1000m
    memory: 2Gi
```

### Configuration with Ingress and TLS

```yaml
# values-ingress.yaml
ingress:
  enabled: true
  className: nginx
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
  hosts:
    - host: blokli.example.com
      paths:
        - path: /
          pathType: Prefix
  tls:
    - secretName: blokli-tls
      hosts:
        - blokli.example.com
```

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

### Configuration with Monitoring

```yaml
# values-monitoring.yaml
monitoring:
  serviceMonitor:
    enabled: true
    interval: 30s
    labels:
      prometheus: kube-prometheus
```

### Configuration with Pod Disruption Budget

```yaml
# values-pdb.yaml
podDisruptionBudget:
  enabled: true
  minAvailable: 1
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
helm repo add bitnami https://charts.bitnami.com/bitnami
helm install postgresql bitnami/postgresql \
  --set auth.username=bloklid \
  --set auth.password=secretpassword \
  --set auth.database=bloklid
```

Initialize the database (bloklid runs migrations automatically on startup).

## Accessing the GraphQL API

### Via Port Forwarding

```bash
kubectl port-forward svc/my-blokli 8080:3064
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

## Testing the Deployment

Run Helm tests to verify the deployment:

```bash
helm test my-blokli
```

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
kubectl get configmap <release-name>-blokli -o yaml
```

### Common Issues

1. **Pod not starting**: Check database connectivity and credentials
2. **OOM errors**: Increase memory limits in resources
3. **Slow indexing**: Increase RPC rate limit or check RPC provider
4. **Disk full**: Increase persistence size

## License

Copyright HOPR Association

## Support

For issues and feature requests, please visit:

- GitHub Issues: <https://github.com/hoprnet/blokli/issues>
- Documentation: <https://github.com/hoprnet/blokli>
