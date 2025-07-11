use std::{
    collections::HashSet,
    env,
    io::{BufRead, BufReader, Cursor},
    path::Path,
    process::Stdio,
};

use cargo_util::paths;
use clap::Parser;
use indexmap::{IndexMap, IndexSet};
use rustfix::{collect_suggestions, CodeFix, Suggestion};
use tracing::{trace, warn};

use crate::{
    core::shell,
    ops::check::{BuildUnit, CheckMessage},
    util::{cli::CheckFlags, package::format_package_id, vcs::VcsOpts},
    CargoResult,
};

#[derive(Debug, Parser)]
pub struct FixitArgs {
    /// Run `clippy` instead of `check`
    #[arg(long)]
    clippy: bool,

    #[command(flatten)]
    vcs_opts: VcsOpts,

    #[command(flatten)]
    check_flags: CheckFlags,
}

impl FixitArgs {
    pub fn exec(self) -> CargoResult<()> {
        exec(self)
    }
}

#[derive(Debug, Default)]
struct File {
    fixes: u32,
}

#[tracing::instrument(skip_all)]
#[allow(clippy::print_stderr)]
fn exec(args: FixitArgs) -> CargoResult<()> {
    args.vcs_opts.valid_vcs()?;

    let mut files = IndexMap::new();

    let max_iterations: usize = env::var("CARGO_FIX_MAX_RETRIES")
        .ok()
        .and_then(|i| i.parse().ok())
        .unwrap_or(4);
    let mut iteration = 0;

    let mut last_errors;

    let mut current_target = None;
    let mut seen = HashSet::new();

    loop {
        trace!("iteration={iteration}");
        trace!("current_target={current_target:?}");
        let (messages, _exit_code) = check(&args)?;

        let (mut errors, build_unit_map) = collect_errors(messages, &seen);

        let mut made_changes = false;

        for (build_unit, file_map) in build_unit_map {
            if !file_map.is_empty()
                && current_target.get_or_insert(build_unit.clone()) == &build_unit
                && fix_errors(&mut files, file_map, &mut errors)?
            {
                made_changes = true;
                break;
            }
        }

        trace!("made_changes={made_changes:?}");
        trace!("current_target={current_target:?}");

        last_errors = errors;
        iteration += 1;

        if !made_changes || iteration >= max_iterations {
            if let Some(pkg) = current_target {
                if seen.iter().all(|b| b.package_id != pkg.package_id) {
                    shell::status("Fixed", format_package_id(&pkg.package_id)?)?;
                }
                seen.insert(pkg);
                current_target = None;
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

fn check(args: &FixitArgs) -> CargoResult<(impl Iterator<Item = CheckMessage>, Option<i32>)> {
    let cmd = if args.clippy { "clippy" } else { "check" };
    let command = std::process::Command::new(env!("CARGO"))
        .args([cmd, "--message-format", "json"])
        .args(args.check_flags.to_flags())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()?;

    let buf = BufReader::new(Cursor::new(command.stdout));

    Ok((
        buf.lines()
            .map_while(|l| l.ok())
            .filter_map(|l| serde_json::from_str(&l).ok()),
        command.status.code(),
    ))
}

#[tracing::instrument(skip_all)]
#[allow(clippy::type_complexity)]
fn collect_errors(
    messages: impl Iterator<Item = CheckMessage>,
    seen: &HashSet<BuildUnit>,
) -> (
    IndexSet<String>,
    IndexMap<BuildUnit, IndexMap<String, IndexSet<(Suggestion, Option<String>)>>>,
) {
    let only = HashSet::new();
    let mut build_unit_map = IndexMap::new();

    let mut errors = IndexSet::new();

    for message in messages {
        let CheckMessage {
            build_unit,
            message: diagnostic,
        } = message;

        if seen.contains(&build_unit) {
            trace!("rejecting build unit `{:?}` already seen", build_unit);
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        }

        let file_map = build_unit_map
            .entry(build_unit.clone())
            .or_insert_with(IndexMap::new);

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

        let mut file_names = suggestion
            .solutions
            .iter()
            .flat_map(|s| s.replacements.iter())
            .map(|r| &r.snippet.file_name);

        let Some(file_name) = file_names.next() else {
            trace!("rejecting as it has no solutions {:?}", suggestion);
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        };

        if !file_names.all(|f| f == file_name) {
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
            .entry(file_name.to_owned())
            .or_insert_with(IndexSet::new)
            .insert((suggestion, diagnostic.rendered));
    }

    (errors, build_unit_map)
}

#[tracing::instrument(skip_all)]
fn fix_errors(
    files: &mut IndexMap<String, File>,
    file_map: IndexMap<String, IndexSet<(Suggestion, Option<String>)>>,
    errors: &mut IndexSet<String>,
) -> CargoResult<bool> {
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

    Ok(made_changes)
}
