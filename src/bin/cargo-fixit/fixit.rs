use std::{
    io::{BufRead, BufReader},
    process::Stdio,
};

use cargo_fixit::CargoResult;
use clap::Parser;
use rustfix::diagnostics::Diagnostic;
use serde::Deserialize;

#[derive(Debug, Parser)]
pub(crate) struct FixitArgs {
    /// Unstable (nightly-only) flags
    #[arg(short = 'Z', value_name = "FLAG")]
    unstable_flags: Vec<String>,
}

impl FixitArgs {
    pub(crate) fn exec(self) -> CargoResult<()> {
        exec(self)
    }
}

#[derive(Deserialize)]
struct CheckMessage {
    message: Diagnostic,
}

fn exec(_args: FixitArgs) -> CargoResult<()> {
    let mut command = std::process::Command::new(env!("CARGO"))
        .args(["check", "--message-format", "json"])
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let buf = BufReader::new(command.stdout.take().expect("could not capture output"));

    for line in buf.lines() {
        let diagnostic = match serde_json::from_str::<CheckMessage>(&line?) {
            Ok(check) => check.message,
            _ => continue,
        };

        if let Some(rendered) = diagnostic.rendered {
            eprint!("{rendered}");
        }
    }

    let _exit_code = command.wait()?;

    Ok(())
}
