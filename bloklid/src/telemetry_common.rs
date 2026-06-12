use std::{
    any::Any,
    backtrace::Backtrace,
    io::stdout,
    panic,
    sync::{Once, OnceLock},
};

use tracing_subscriber::{Registry, prelude::*, reload};

use crate::errors::{BloklidError, Result};

pub(crate) type OtelBoxedLayer = Box<dyn tracing_subscriber::Layer<Registry> + Send + Sync + 'static>;

static OTEL_HANDLE: OnceLock<reload::Handle<Vec<OtelBoxedLayer>, Registry>> = OnceLock::new();
static PANIC_HOOK_INSTALLED: Once = Once::new();

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
    install_panic_hook();
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

fn install_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        panic::set_hook(Box::new(|info| {
            let payload = panic_payload_to_string(info.payload());
            let location = info.location();
            let panic_file = location.map(|value| value.file()).unwrap_or("unknown");
            let panic_line = location.map(|value| value.line()).unwrap_or(0);
            let panic_column = location.map(|value| value.column()).unwrap_or(0);
            let thread = std::thread::current();
            let thread_name = thread.name().unwrap_or("unnamed");
            let backtrace = Backtrace::force_capture().to_string();

            tracing::error!(
                panic_payload = %payload,
                panic_file,
                panic_line,
                panic_column,
                thread_name,
                thread_id = ?thread.id(),
                backtrace = %backtrace,
                "process panic"
            );
        }));
    });
}

fn panic_payload_to_string(payload: &(dyn Any + Send)) -> String {
    if let Some(payload) = payload.downcast_ref::<&str>() {
        (*payload).to_string()
    } else if let Some(payload) = payload.downcast_ref::<String>() {
        payload.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_otel_layers_fails_before_base_subscriber_is_initialized() {
        let error = install_otel_layers(Vec::new()).expect_err("otel layers should require a base subscriber");

        assert!(error.to_string().contains("base subscriber not initialized"));
    }

    #[test]
    fn test_panic_payload_to_string_from_str() {
        let payload: &(dyn Any + Send) = &"boom";
        assert_eq!(panic_payload_to_string(payload), "boom");
    }

    #[test]
    fn test_panic_payload_to_string_from_string() {
        let payload: &(dyn Any + Send) = &"boom".to_string();
        assert_eq!(panic_payload_to_string(payload), "boom");
    }
}
