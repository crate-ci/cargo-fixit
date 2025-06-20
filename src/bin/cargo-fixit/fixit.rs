use std::{
    collections::HashSet,
    env,
    io::{BufRead, BufReader},
    path::Path,
    process::Stdio,
};

use cargo_fixit::CargoResult;
use cargo_util::paths;
use clap::Parser;
use indexmap::{IndexMap, IndexSet};
use rustfix::{collect_suggestions, diagnostics::Diagnostic, CodeFix};
use serde::Deserialize;
use tracing::{trace, warn};

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
    run_rustfix()
}

fn run_rustfix() -> CargoResult<()> {
    let only = HashSet::new();
    let mut file_map = IndexMap::new();

    let mut errors = IndexSet::new();

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
        let filter = if env::var("__CARGO_FIX_YOLO").is_ok() {
            rustfix::Filter::Everything
        } else {
            rustfix::Filter::MachineApplicableOnly
        };

        let Some(suggestion) = collect_suggestions(&diagnostic, &only, filter) else {
            trace!("rejecting as not a MachineApplicable diagnosis: {diagnostic:?}");
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        };

        let file_names = suggestion
            .solutions
            .iter()
            .flat_map(|s| s.replacements.iter())
            .map(|r| &r.snippet.file_name);

        let file_name = if let Some(file_name) = file_names.clone().next() {
            file_name.clone()
        } else {
            trace!("rejecting as it has no solutions {:?}", suggestion);
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        };

        if !file_names.clone().all(|f| f == &file_name) {
            trace!("rejecting as it changes multiple files: {:?}", suggestion);
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        }

        let file_path = Path::new(&file_name);
        // Do not write into registry cache. See rust-lang/cargo#9857.
        if let Ok(home) = env::var("CARGO_HOME") {
            if file_path.starts_with(home) {
                continue;
            }
        }

        file_map
            .entry(file_name)
            .or_insert_with(IndexSet::new)
            .insert((suggestion, diagnostic.rendered));
    }

    let _exit_code = command.wait()?;

    for (file, suggestions) in file_map {
        let code = match paths::read(file.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                warn!("failed to read `{}`: {}", file, e);
                errors.extend(suggestions.iter().filter_map(|(_, e)| e.clone()));
                continue;
            }
        };

        let mut fixed = CodeFix::new(&code);
        let mut num_fixes = 0;

        for (suggestion, rendered) in suggestions {
            match fixed.apply(&suggestion) {
                Ok(()) => num_fixes += 1,
                Err(rustfix::Error::AlreadyReplaced {
                    is_identical: true, ..
                }) => {}
                Err(e) => {
                    if let Some(rendered) = rendered {
                        errors.insert(rendered);
                    }
                    warn!("{e:?}");
                }
            }
        }
        if fixed.modified() {
            eprintln!("{file}: {num_fixes} fixes");
            let new_code = fixed.finish()?;
            paths::write(&file, new_code)?;
        }
    }

    for e in errors {
        eprint!("{e}");
    }

    Ok(())
}
