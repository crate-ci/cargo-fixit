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
    core::{shell, sysroot::get_sysroot},
    ops::check::{BuildUnit, CheckOutput, DiagnosticLevel, Message, MessageDiagnostic},
    util::{
        cli::CheckFlags, messages::gen_please_report_this_bug_text, package::format_package_id,
        vcs::VcsOpts,
    },
    CargoResult,
};

#[derive(Debug, Parser)]
pub struct FixitArgs {
    /// Run `clippy` instead of `check`
    #[arg(long)]
    clippy: bool,

    /// Fix code even if it already has compiler errors
    #[arg(long)]
    broken_code: bool,

    #[command(flatten)]
    color: colorchoice_clap::Color,

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
    original_source: String,
}

#[tracing::instrument(skip_all)]
fn exec(args: FixitArgs) -> CargoResult<()> {
    args.color.write_global();

    args.vcs_opts.valid_vcs()?;

    let mut files: IndexMap<String, File> = IndexMap::new();

    let max_iterations: usize = env::var("CARGO_FIX_MAX_RETRIES")
        .ok()
        .and_then(|i| i.parse().ok())
        .unwrap_or(4);
    let mut iteration = 0;
    let mut lint_cap = false;

    let mut last_errors = IndexMap::new();
    let mut current_target: Option<BuildUnit> = None;
    let mut seen = HashSet::new();

    loop {
        trace!("iteration={iteration}");
        trace!("current_target={current_target:?}");
        let (messages, exit_code) = check(&args, &mut lint_cap)?;

        if !args.broken_code && exit_code != Some(0) {
            let mut out = String::new();

            if current_target.is_some() {
                out.push_str(
                    "failed to automatically apply fixes suggested by rustc\n\n\
                    after fixes were automatically applied the \
                    compiler reported errors within these files:\n\n",
                );

                for (
                    file,
                    File {
                        fixes: _,
                        original_source,
                    },
                ) in &files
                {
                    out.push_str(&format!("  * {file}\n"));
                    shell::note(format!("reverting `{file}` to its original state"))?;
                    paths::write(file, original_source)?;
                }
                out.push('\n');

                out.push_str(&gen_please_report_this_bug_text(args.clippy));

                let mut errors = messages
                    .into_iter()
                    .filter_map(|e| match e {
                        CheckOutput::Message(m) => m.message.diagnostic.rendered,
                        _ => None,
                    })
                    .peekable();
                if errors.peek().is_some() {
                    out.push_str("The errors reported are:\n");
                }

                for e in errors {
                    out.push_str(&format!("{}\n\n", e.trim_end()));
                }

                let (messages, _) = check(&args, &mut lint_cap)?;
                let mut errors = messages
                    .into_iter()
                    .filter_map(|e| match e {
                        CheckOutput::Message(m) => m.message.diagnostic.rendered,
                        _ => None,
                    })
                    .peekable();

                if errors.peek().is_some() {
                    out.push_str("The original errors are:\n");
                }

                for e in errors {
                    out.push_str(&format!("{}\n\n", e.trim_end()));
                }

                shell::warn(out)?;
            } else {
                for e in messages.into_iter().filter_map(|e| match e {
                    CheckOutput::Message(m) => m.message.diagnostic.rendered,
                    _ => None,
                }) {
                    shell::print_ansi_stderr(format!("{}\n\n", e.trim_end()).as_bytes())?;
                }
            }

            shell::note("try using `--broken-code` to fix errors")?;
            anyhow::bail!("could not compile");
        }

        let (mut errors, mut build_unit_map) = collect_errors(messages.into_iter(), &seen);

        if iteration >= max_iterations {
            if let Some(target) = current_target {
                if let Some(file_map) = build_unit_map.get(&target) {
                    let target_errors = errors.entry(target.clone()).or_default();
                    target_errors.extend(
                        file_map
                            .values()
                            .flatten()
                            .filter_map(|(_, diagnostic)| diagnostic.clone()),
                    );
                }
                finish_target(target, &mut files, &mut errors, &mut seen)?;
                current_target = None;
                iteration = 0;
            } else {
                break;
            }
        }

        let mut finalized_target = false;
        if let Some(target) = current_target.as_ref() {
            if build_unit_map.get(target).is_none_or(IndexMap::is_empty) {
                let target = current_target.take().expect("current target is present");
                build_unit_map.shift_remove(&target);
                finish_target(target, &mut files, &mut errors, &mut seen)?;
                iteration = 0;
                finalized_target = true;
            }
        }

        let mut made_changes = false;

        for (build_unit, file_map) in build_unit_map {
            if seen.contains(&build_unit) {
                continue;
            }

            let build_unit_errors = errors
                .entry(build_unit.clone())
                .or_insert_with(IndexSet::new);

            if current_target.is_none() && file_map.is_empty() {
                if finalized_target && build_unit_errors.is_empty() {
                    continue;
                }
                if seen.iter().all(|b| b.package_id != build_unit.package_id) {
                    shell::status("Checking", format_package_id(&build_unit.package_id)?)?;
                }
                for e in build_unit_errors.iter() {
                    shell::print_ansi_stderr(format!("{}\n\n", e.trim_end()).as_bytes())?;
                }
                errors.shift_remove(&build_unit);

                seen.insert(build_unit);
            } else if !file_map.is_empty()
                && current_target.get_or_insert(build_unit.clone()) == &build_unit
                && fix_errors(&mut files, file_map, build_unit_errors)?
            {
                made_changes = true;
                break;
            }
        }

        trace!("made_changes={made_changes:?}");
        trace!("current_target={current_target:?}");

        last_errors = errors;
        iteration += 1;

        if !made_changes {
            if let Some(pkg) = current_target {
                finish_target(pkg, &mut files, &mut last_errors, &mut seen)?;
                current_target = None;
                iteration = 0;
                continue;
            }
            break;
        }
    }

    for (name, file) in files {
        shell::fixed(name, file.fixes)?;
    }

    for e in last_errors.iter().flat_map(|(_, e)| e) {
        shell::print_ansi_stderr(format!("{}\n\n", e.trim_end()).as_bytes())?;
    }

    Ok(())
}

/// Marks a target complete after reporting its fixes and remaining diagnostics.
fn finish_target(
    target: BuildUnit,
    files: &mut IndexMap<String, File>,
    errors: &mut IndexMap<BuildUnit, IndexSet<String>>,
    seen: &mut HashSet<BuildUnit>,
) -> CargoResult<()> {
    if seen
        .iter()
        .all(|build_unit| build_unit.package_id != target.package_id)
    {
        shell::status("Checking", format_package_id(&target.package_id)?)?;
    }

    for (name, file) in std::mem::take(files) {
        shell::fixed(name, file.fixes)?;
    }

    for error in errors.shift_remove(&target).unwrap_or_default() {
        shell::print_ansi_stderr(format!("{}\n\n", error.trim_end()).as_bytes())?;
    }

    seen.insert(target);
    Ok(())
}

fn check(args: &FixitArgs, lint_cap: &mut bool) -> CargoResult<(Vec<CheckOutput>, Option<i32>)> {
    let cmd = if args.clippy { "clippy" } else { "check" };
    let mut command = std::process::Command::new(env!("CARGO"));
    command
        .args([cmd, "--message-format", "json-diagnostic-rendered-ansi"])
        .args(args.check_flags.to_flags())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped());
    if *lint_cap {
        cap_lints(&mut command);
    }
    let output = command.output()?;
    let mut output = to_check_output(output);

    if output.1 != Some(0) && !*lint_cap && denied_lint(&output.0) {
        *lint_cap = true;
        cap_lints(&mut command);
        output = to_check_output(command.output()?);
    }

    Ok(output)
}

/// Applies the original lint cap while preserving existing compiler flags.
fn cap_lints(command: &mut std::process::Command) {
    if let Ok(flags) = env::var("CARGO_ENCODED_RUSTFLAGS") {
        let separator = if flags.is_empty() { "" } else { "\u{1f}" };
        command.env(
            "CARGO_ENCODED_RUSTFLAGS",
            format!("{flags}{separator}--cap-lints=warn"),
        );
    } else {
        command.env(
            "RUSTFLAGS",
            format!(
                "--cap-lints=warn {}",
                env::var("RUSTFLAGS").unwrap_or("".to_owned())
            ),
        );
    }
}

fn denied_lint(messages: &[CheckOutput]) -> bool {
    messages.iter().any(|message| {
        matches!(&message, CheckOutput::Message(message)
                if message.message.level == DiagnosticLevel::Error
                    && message.message.diagnostic.code.is_some())
    })
}

fn to_check_output(output: std::process::Output) -> (Vec<CheckOutput>, Option<i32>) {
    let buf = BufReader::new(Cursor::new(output.stdout));
    (
        buf.lines()
            .map_while(|l| l.ok())
            .filter_map(|l| serde_json::from_str(&l).ok())
            .collect(),
        output.status.code(),
    )
}

#[tracing::instrument(skip_all)]
#[allow(clippy::type_complexity)]
fn collect_errors(
    messages: impl Iterator<Item = CheckOutput>,
    seen: &HashSet<BuildUnit>,
) -> (
    IndexMap<BuildUnit, IndexSet<String>>,
    IndexMap<BuildUnit, IndexMap<String, IndexSet<(Suggestion, Option<String>)>>>,
) {
    let only = HashSet::new();
    let mut build_unit_map = IndexMap::new();

    let mut errors = IndexMap::new();

    for message in messages {
        let Message {
            build_unit,
            message: MessageDiagnostic { diagnostic, .. },
        } = match message {
            CheckOutput::Message(m) => m,
            CheckOutput::Artifact(a) => {
                if !seen.contains(&a.build_unit) && !a.fresh {
                    build_unit_map
                        .entry(a.build_unit.clone())
                        .or_insert(IndexMap::new());
                }
                continue;
            }
        };

        let errors = errors
            .entry(build_unit.clone())
            .or_insert_with(IndexSet::new);

        if seen.contains(&build_unit) {
            trace!("rejecting build unit `{:?}` already seen", build_unit);
            continue;
        }

        let file_map = build_unit_map
            .entry(build_unit.clone())
            .or_insert(IndexMap::new());

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
                if let Some(rendered) = diagnostic.rendered {
                    errors.insert(rendered);
                }
                continue;
            }
        }

        if file_path.is_absolute() {
            if let Some(sysroot) = get_sysroot() {
                if file_path.starts_with(sysroot) {
                    if let Some(rendered) = diagnostic.rendered {
                        errors.insert(rendered);
                    }
                    continue;
                }
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
        let source = match paths::read(file.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                warn!("failed to read `{}`: {}", file, e);
                errors.extend(suggestions.iter().filter_map(|(_, e)| e.clone()));
                continue;
            }
        };

        let mut fixed = CodeFix::new(&source);
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
            let new_source = fixed.finish()?;
            let file_state = files.entry(file.clone()).or_insert(File {
                fixes: 0,
                original_source: source,
            });
            paths::write(&file, new_source)?;
            made_changes = true;
            file_state.fixes += num_fixes;
        }
    }

    Ok(made_changes)
}
