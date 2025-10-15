//! GraphQL schema definitions for blokli API

use async_graphql::dynamic::*;
use sea_orm::DatabaseConnection;
use seaography::{Builder, BuilderContext};

/// Build the seaography-powered GraphQL schema with database connection
///
/// This creates a dynamic GraphQL schema with:
/// - Auto-generated queries for public entities (account, announcement, channel)
/// - Auto-generated mutations
/// - Custom health and version queries
///
/// Note: Only public-facing entities are exposed. Internal entities like log_status,
/// chain_info, and node_info are not accessible through the GraphQL API.
pub fn build_schema(db: DatabaseConnection) -> Result<Schema, Box<dyn std::error::Error>> {
    // Create static builder context (required by seaography)
    let context: &'static BuilderContext = Box::leak(Box::new(BuilderContext::default()));

    // Create seaography builder with database connection
    let builder = Builder::new(context, db.clone());

    // Register only public-facing entities
    let mut builder = blokli_db_entity::register_public_entities(builder);

    // Add custom query fields before building schema
    let health_field = Field::new("health", TypeRef::named_nn(TypeRef::STRING), |_ctx| {
        FieldFuture::new(async move { Ok(Some(FieldValue::value("ok"))) })
    })
    .description("Health check endpoint");

    let version_field = Field::new("version", TypeRef::named_nn(TypeRef::STRING), |_ctx| {
        FieldFuture::new(async move { Ok(Some(FieldValue::value(env!("CARGO_PKG_VERSION")))) })
    })
    .description("API version");

    // Add fields directly to the query object
    builder.query = builder.query.field(health_field).field(version_field);

    // Build the schema
    let schema = builder.schema_builder().finish()?;

    Ok(schema)
}

/// Export the GraphQL schema to SDL (Schema Definition Language) format
///
/// This generates a string representation of the GraphQL schema that can be used
/// for code generation, documentation, or schema validation tools.
pub fn export_schema_sdl(db: DatabaseConnection) -> Result<String, Box<dyn std::error::Error>> {
    let schema = build_schema(db)?;
    Ok(schema.sdl())
}
