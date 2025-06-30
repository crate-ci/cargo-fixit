use std::{
    collections::HashSet,
    env,
    io::{BufRead, BufReader},
    path::Path,
    process::Stdio,
};

use cargo_fixit::{shell, CargoResult, CheckFlags, CheckMessage};
use cargo_util::paths;
use clap::Parser;
use indexmap::{IndexMap, IndexSet};
use rustfix::{collect_suggestions, CodeFix};
use tracing::{trace, warn};

#[derive(Debug, Parser)]
pub(crate) struct FixitArgs {
    /// Fix code even if a VCS was not detected
    #[arg(long)]
    allow_no_vcs: bool,

    /// Run `clippy` instead of `check`
    #[arg(long)]
    clippy: bool,

    #[command(flatten)]
    check_flags: CheckFlags,
}

impl FixitArgs {
    pub(crate) fn exec(self) -> CargoResult<()> {
        exec(self)
    }
}

#[derive(Debug, Default)]
struct File {
    fixes: u32,
}

fn exec(args: FixitArgs) -> CargoResult<()> {
    if !args.allow_no_vcs {
        shell::warn("support for VCS has not been implemented")?;
    }
    let mut files = IndexMap::new();

    let max_iterations: usize = env::var("CARGO_FIX_MAX_RETRIES")
        .ok()
        .and_then(|i| i.parse().ok())
        .unwrap_or(4);
    let mut iteration = 0;

    let mut last_errors;

    let mut current_package_id = None;
    let mut seen = HashSet::new();

    loop {
        let (errors, made_changes) =
            run_rustfix(&args, &mut files, &mut current_package_id, &seen)?;

        last_errors = errors;
        iteration += 1;

        if !made_changes || iteration >= max_iterations {
            if let Some(pkg) = current_package_id {
                seen.insert(pkg);
                current_package_id = None;
                iteration = 0;
            } else {
                break;
            }
        }
    }
    for (name, file) in files {
        shell::fixed(name, file.fixes)?;
    }

    for e in last_errors {
        eprint!("{}\n\n", e.trim_end());
    }

    Ok(())
}

fn run_rustfix(
    args: &FixitArgs,
    files: &mut IndexMap<String, File>,
    current_package_id: &mut Option<String>,
    seen: &HashSet<String>,
) -> CargoResult<(IndexSet<String>, bool)> {
    let only = HashSet::new();
    let mut file_map = IndexMap::new();

    let mut errors = IndexSet::new();

    let cmd = if args.clippy { "clippy" } else { "check" };
    let mut command = std::process::Command::new(env!("CARGO"))
        .args([cmd, "--message-format", "json"])
        .args(args.check_flags.to_flags())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    let buf = BufReader::new(command.stdout.take().expect("could not capture output"));

    for line in buf.lines() {
        let Ok(CheckMessage {
            message: diagnostic,
            package_id,
        }) = serde_json::from_str(&line?)
        else {
            continue;
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

        if seen.contains(&package_id) {
            trace!(
                "rejecting package id `{}` already seen: {:?}",
                package_id,
                suggestion,
            );
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        }

        let current_package_id = current_package_id.get_or_insert(package_id.clone());

        if current_package_id == &package_id {
            file_map
                .entry(file_name)
                .or_insert_with(IndexSet::new)
                .insert((suggestion, diagnostic.rendered));
        }
    }

    let _exit_code = command.wait()?;

    let mut made_changes = false;
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

        for (suggestion, rendered) in suggestions.iter().rev() {
            match fixed.apply(suggestion) {
                Ok(()) => num_fixes += 1,
                Err(rustfix::Error::AlreadyReplaced {
                    is_identical: true, ..
                }) => {}
                Err(e) => {
                    if let Some(rendered) = rendered {
                        errors.insert(rendered.to_owned());
                    }
                    warn!("{e:?}");
                }
            }
        }
        if fixed.modified() {
            let new_code = fixed.finish()?;
            paths::write(&file, new_code)?;
            made_changes = true;
            files.entry(file).or_default().fixes += num_fixes;
        }
    }

    Ok((errors, made_changes))
}
