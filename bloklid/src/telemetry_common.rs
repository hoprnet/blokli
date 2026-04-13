use std::{io::stdout, sync::OnceLock};

use tracing_subscriber::{Registry, prelude::*, reload};

use crate::errors::{BloklidError, Result};

pub(crate) type OtelBoxedLayer = Box<dyn tracing_subscriber::Layer<Registry> + Send + Sync + 'static>;

static OTEL_HANDLE: OnceLock<reload::Handle<Vec<OtelBoxedLayer>, Registry>> = OnceLock::new();

fn passthrough_layers(layers: Vec<OtelBoxedLayer>) -> Vec<OtelBoxedLayer> {
    if layers.is_empty() {
        vec![Box::new(tracing_subscriber::layer::Identity::new())]
    } else {
        layers
    }
}

pub(crate) fn install_base_subscriber(verbosity: u8) -> Result<()> {
    let env_filter = if std::env::var(tracing_subscriber::EnvFilter::DEFAULT_ENV).is_ok() {
        tracing_subscriber::EnvFilter::from_default_env()
    } else {
        match verbosity {
            0 => tracing_subscriber::EnvFilter::new("info"),
            1 => tracing_subscriber::EnvFilter::new("debug"),
            _ => tracing_subscriber::EnvFilter::new("trace"),
        }
    };

    let format = tracing_subscriber::fmt::layer()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(false)
        .with_writer(stdout);
    let format = if std::env::var("BLOKLI_LOG_FORMAT")
        .map(|value| value.eq_ignore_ascii_case("json"))
        .unwrap_or(false)
    {
        format.json().boxed()
    } else {
        format.boxed()
    };

    let (reload_layer, handle) = reload::Layer::<Vec<OtelBoxedLayer>, Registry>::new(passthrough_layers(Vec::new()));
    let _ = OTEL_HANDLE.set(handle);

    let subscriber = Registry::default().with(reload_layer).with(env_filter).with(format);

    tracing::subscriber::set_global_default(subscriber)
        .map_err(|error| BloklidError::NonSpecific(error.to_string()))?;
    Ok(())
}

pub(crate) fn install_otel_layers(layers: Vec<OtelBoxedLayer>) -> Result<()> {
    OTEL_HANDLE
        .get()
        .ok_or_else(|| BloklidError::NonSpecific("base subscriber not initialized".into()))?
        .reload(passthrough_layers(layers))
        .map_err(|error| BloklidError::NonSpecific(error.to_string()))?;
    Ok(())
}
