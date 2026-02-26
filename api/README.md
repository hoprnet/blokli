# blokli-api

GraphQL API server for HOPR blokli indexer built with Axum and async-graphql.

## Features

- **GraphQL API**: Full GraphQL query and mutation support
- **Subscriptions**: Real-time subscriptions using Server-Sent Events (SSE)
- **HTTP/2**: High-performance HTTP/2 support
- **TLS 1.3**: Secure connections with TLS 1.3 (when configured)
- **GraphQL Playground**: Interactive GraphQL IDE for development
- **CORS**: Configured for cross-origin requests
- **Compression**: Zstandard (zstd) compression only for responses >1KB
- **Logging**: Structured logging with tracing

## Running the Server

### Development

```bash
cargo run -p blokli-api
```

### Production

```bash
cargo run --release -p blokli-api
```

## Endpoints

- **GraphQL API**: `http://localhost:8080/graphql` (GET for playground, POST for queries)
- **GraphQL Subscriptions**: `http://localhost:8080/graphql/subscriptions` (SSE)
- **Health Check**: `http://localhost:8080/health`

## GraphQL Schema

### Queries

- `health`: Health check endpoint
- `version`: Get API version
- `blocks(limit: Int)`: Get indexed blocks (placeholder)

### Mutations

- `placeholder`: Placeholder mutation (to be implemented)

### Subscriptions

- `newBlocks`: Subscribe to new block events

## Configuration

The server can be configured via the `ApiConfig` struct:

```rust
use blokli_api::config::{ApiConfig, TlsConfig};
use std::path::PathBuf;

// Without TLS
let config = ApiConfig {
    bind_address: "0.0.0.0:8080".parse().unwrap(),
    playground_enabled: true,
    gas_multiplier: 1.0,
    tls: None,
    ..Default::default()
};

// With TLS 1.3
let config = ApiConfig {
    bind_address: "0.0.0.0:8443".parse().unwrap(),
    playground_enabled: true,
    gas_multiplier: 1.0,
    tls: Some(TlsConfig {
        cert_path: PathBuf::from("/path/to/cert.pem"),
        key_path: PathBuf::from("/path/to/key.pem"),
    }),
    ..Default::default()
};
```

### TLS Configuration

When TLS is enabled:

- Only TLS 1.3 is supported for maximum security
- HTTP/2 and HTTP/1.1 are advertised via ALPN
- Certificate and private key must be in PEM format

## Environment Variables

- `RUST_LOG`: Configure logging level (default: `blokli_api=info,tower_http=debug`)

## Example Queries

### Health Check

```graphql
query {
  health
}
```

### Get Version

```graphql
query {
  version
}
```

### Get Blocks

```graphql
query {
  blocks(limit: 10) {
    number
    hash
    timestamp
  }
}
```

### Subscribe to New Blocks

```graphql
subscription {
  newBlocks {
    number
    hash
    timestamp
  }
}
```

## Development

The API is designed to be extended with additional GraphQL types and resolvers. The schema is defined in
`src/schema.rs`.

## License

GPL-3.0-only
