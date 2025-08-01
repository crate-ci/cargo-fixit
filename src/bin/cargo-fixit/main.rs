use cargo_fixit::core::shell;
use clap::Parser as _;
use std::ffi::OsStr;

mod cli;

fn main() {
    let _guard = setup_logger();

    let args = cli::Command::parse();

    if let Err(err) = args.exec() {
        shell::error(&err).unwrap();

        std::process::exit(1);
    }
}

fn setup_logger() -> Option<ChromeFlushGuard> {
    use tracing_subscriber::prelude::*;

    let env = tracing_subscriber::EnvFilter::from_env("FIXIT_LOG");
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_timer(tracing_subscriber::fmt::time::Uptime::default())
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .with_filter(env);

    let (profile_layer, profile_guard) = chrome_layer();

    let registry = tracing_subscriber::registry()
        .with(fmt_layer)
        .with(profile_layer);
    registry.init();
    profile_guard
}

#[cfg(target_has_atomic = "64")]
type ChromeFlushGuard = tracing_chrome::FlushGuard;
#[cfg(target_has_atomic = "64")]
fn chrome_layer<S>() -> (
    Option<tracing_chrome::ChromeLayer<S>>,
    Option<ChromeFlushGuard>,
)
where
    S: tracing::Subscriber
        + for<'span> tracing_subscriber::registry::LookupSpan<'span>
        + Send
        + Sync,
{
    #![allow(clippy::disallowed_methods)]

    if env_to_bool(std::env::var_os("CARGO_LOG_PROFILE").as_deref()) {
        let capture_args =
            env_to_bool(std::env::var_os("CARGO_LOG_PROFILE_CAPTURE_ARGS").as_deref());
        let (layer, guard) = tracing_chrome::ChromeLayerBuilder::new()
            .include_args(capture_args)
            .build();
        (Some(layer), Some(guard))
    } else {
        (None, None)
    }
}

#[cfg(not(target_has_atomic = "64"))]
type ChromeFlushGuard = ();
#[cfg(not(target_has_atomic = "64"))]
fn chrome_layer() -> (
    Option<tracing_subscriber::layer::Identity>,
    Option<ChromeFlushGuard>,
) {
    (None, None)
}

#[cfg(target_has_atomic = "64")]
fn env_to_bool(os: Option<&OsStr>) -> bool {
    matches!(os.and_then(|os| os.to_str()), Some("1") | Some("true"))
}
