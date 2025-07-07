#![allow(dead_code)]

use anyhow::Context;
use clap::builder::styling::Style;
use clap_cargo::style::{ERROR, HEADER, NOTE, WARN};
use std::io::Write;

use crate::CargoResult;

/// Print a message with a colored title in the style of Cargo shell messages.
pub fn print(
    status: &str,
    message: impl std::fmt::Display,
    style: Style,
    justified: bool,
) -> CargoResult<()> {
    let mut stderr = anstream::stderr().lock();
    if justified {
        write!(stderr, "{style}{status:>12}{style:#}")?;
    } else {
        write!(stderr, "{style}{status}{style:#}:")?;
    }

    writeln!(stderr, " {message:#}").with_context(|| "Failed to write message")?;

    Ok(())
}

/// Print a styled action message.
pub fn status(action: &str, message: impl std::fmt::Display) -> CargoResult<()> {
    print(action, message, HEADER, true)
}

/// Print a styled error message.
pub fn error(message: impl std::fmt::Display) -> CargoResult<()> {
    print("error", message, ERROR, false)
}

/// Print a styled warning message.
pub fn warn(message: impl std::fmt::Display) -> CargoResult<()> {
    print("warning", message, WARN, false)
}

/// Print a styled warning message.
pub fn note(message: impl std::fmt::Display) -> CargoResult<()> {
    print("note", message, NOTE, false)
}

/// Print a styled fixed message
pub fn fixed(file_name: impl std::fmt::Display, fixes: u32) -> CargoResult<()> {
    status(
        "Fixed",
        format!(
            "{file_name} ({fixes} {})",
            if fixes == 1 { "fix" } else { "fixes" }
        ),
    )
}
