use std::{
    any::Any,
    backtrace::Backtrace,
    io::stdout,
    panic,
    sync::{Once, OnceLock},
};

use tracing_subscriber::{Layer as _, Registry, layer::SubscriberExt as _, util::SubscriberInitExt as _};

static PANIC_HOOK_INSTALLED: Once = Once::new();
static TRACING_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn install_tracing(default_env_filter: &str) -> Result<(), String> {
    TRACING_INIT
        .get_or_init(|| try_install_tracing(default_env_filter))
        .clone()?;
    install_panic_hook();
    Ok(())
}

fn try_install_tracing(default_env_filter: &str) -> Result<(), String> {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_env_filter));

    let format = tracing_subscriber::fmt::layer()
        .with_level(true)
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(false)
        .with_writer(stdout);
    let subscriber = Registry::default().with(env_filter).with(
        if std::env::var("BLOKLI_LOG_FORMAT")
            .map(|value| value.eq_ignore_ascii_case("json"))
            .unwrap_or(false)
        {
            format.json().boxed()
        } else {
            format.boxed()
        },
    );

    subscriber.try_init().map_err(|error| error.to_string())
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
