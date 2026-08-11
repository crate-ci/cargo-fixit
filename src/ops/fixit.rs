use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Cursor;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

use anyhow::Context;
use cargo_metadata::Metadata;
use cargo_metadata::MetadataCommand;
use cargo_util::paths;
use cargo_util_schemas::core::PackageIdSpec;
use clap::ArgAction;
use clap::Parser;
use indexmap::{IndexMap, IndexSet};
use rustfix::{collect_suggestions, CodeFix, Suggestion};
use tracing::{trace, warn};

use crate::util::cli::PackageSelection;
use crate::{
    core::{shell, sysroot::get_sysroot},
    ops::check::{
        BuildUnit, CheckOutput, CrateType, DiagnosticLevel, Message, MessageDiagnostic, TargetKind,
    },
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

    /// Fix all targets together, risking stale suggestions
    #[arg(long = "Zdangerous-parallel-fixes")]
    dangerous_parallel_fixes: bool,

    #[command(flatten)]
    color: colorchoice_clap::Color,

    #[command(flatten)]
    vcs_opts: VcsOpts,

    #[command(flatten)]
    check_flags: CheckFlags,

    #[arg(long, action = ArgAction::Count)]
    verbose: u8,
}

impl FixitArgs {
    pub fn exec(self) -> CargoResult<()> {
        exec(self)
    }

    fn to_command(&self) -> Command {
        let cmd = if self.clippy { "clippy" } else { "check" };
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut command = Command::new(cargo);
        command.arg(cmd).args(self.check_flags.to_flags());
        command
    }
}

#[derive(Debug, Default)]
struct ActiveState {
    snapshots: IndexMap<String, File>,
    iterations: usize,
}

#[derive(Debug, Default)]
struct File {
    fixes: u32,
    original_source: String,
}

type BuildUnitErrors = IndexMap<UnitId, IndexSet<String>>;
type BuildUnitSuggestions =
    IndexMap<UnitId, IndexMap<String, IndexSet<(Suggestion, Option<String>)>>>;

#[tracing::instrument(skip_all)]
fn exec(args: FixitArgs) -> CargoResult<()> {
    args.color.write_global();

    args.vcs_opts.valid_vcs()?;

    let mut active_units = IndexMap::new();
    match fix(&args, &mut active_units) {
        Ok(()) => Ok(()),
        Err(error) => {
            for (file, original) in active_units
                .values()
                .flat_map(|state| state.snapshots.iter())
            {
                paths::write(file, &original.original_source)?;
            }
            Err(error)
        }
    }
}

fn fix(args: &FixitArgs, active_units: &mut IndexMap<UnitId, ActiveState>) -> CargoResult<()> {
    let max_iterations: usize = env::var("CARGO_FIX_MAX_RETRIES")
        .ok()
        .and_then(|i| i.parse().ok())
        .unwrap_or(4);
    let package_metadata = package_metadata(&args.check_flags)?;
    let primary_packages = PrimaryPackages::from_metadata(&package_metadata, &args.check_flags)?;
    let mut plan = if args.dangerous_parallel_fixes {
        UnitGraph::flat(&package_metadata)
    } else {
        UnitGraph::new(&package_metadata)
    };
    trace!("plan `{plan:#?}`");

    let mut lint_cap = false;
    let mut seen = BTreeSet::new();
    let mut first = true;
    let mut claimed_files: HashMap<same_file::Handle, UnitId> = HashMap::new();
    loop {
        trace!("check ({active_units:?})");
        let (mut messages, exit_code) = check(args, &mut lint_cap)?;
        messages.sort_unstable_by_key(|m| m.build_unit().cloned());
        print_built(args, &messages)?;

        if messages.is_empty() && exit_code != Some(0) {
            let mut command = args.to_command();
            command.status()?;
            anyhow::bail!("could not compile");
        } else if !args.broken_code && exit_code != Some(0) {
            let mut out = String::new();

            if !active_units.is_empty() {
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
                ) in active_units
                    .values()
                    .flat_map(|state| state.snapshots.iter())
                {
                    out.push_str(&format!("  * {file}\n"));
                    shell::note(format!("reverting `{file}` to its original state"))?;
                    paths::write(file, original_source)?;
                }
                active_units.clear();
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

                let (messages, _) = check(args, &mut lint_cap)?;
                print_built(args, &messages)?;
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
        if first {
            first = false;
            let mut errors = IndexMap::new();
            for message in &messages {
                match message {
                    CheckOutput::Message(Message {
                        build_unit,
                        message: MessageDiagnostic { diagnostic, .. },
                    }) => {
                        let unit_id = UnitId::from_message(build_unit);
                        if let Some(rendered) = diagnostic.rendered.clone() {
                            let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
                            errors.insert(rendered);
                        }
                    }
                    CheckOutput::Artifact(a) => {
                        let package_id = &a.build_unit.package_id;
                        let unit_id = UnitId::from_message(&a.build_unit);
                        if !is_local(package_id) || !plan.dependencies.contains_key(&unit_id) {
                            for error in errors.get(&unit_id).into_iter().flatten() {
                                shell::print_ansi_stderr(
                                    format!("{}\n\n", error.trim_end()).as_bytes(),
                                )?;
                            }
                            if !a.fresh && seen.insert(package_id.to_owned()) {
                                shell::status("Checking", format_package_id(package_id)?)?;
                            }
                        }
                    }
                }
            }
        }

        let observed_packages: HashSet<String> = messages
            .iter()
            .filter_map(CheckOutput::build_unit)
            .map(|unit| unit.package_id.clone())
            .collect();
        let (mut errors, suggestions) = collect_diagnostics(
            messages.into_iter(),
            &plan.finished,
            &primary_packages,
            active_units,
            max_iterations,
        );

        let mut finishing = true;
        while finishing {
            let mut finished = BTreeSet::new();
            for unit_id in active_units.keys() {
                if suggestions.contains_key(unit_id) {
                    continue;
                }
                let errors = errors.shift_remove(unit_id);
                finish_unit(unit_id, active_units, errors.as_ref())?;
                finished.insert(unit_id.clone());
            }
            active_units.retain(|k, _v| !finished.contains(k));
            claimed_files.retain(|_k, v| !finished.contains(v));
            plan.mark_finished(finished);
            finishing = false;
            for unit_id in plan.take_ready() {
                finishing = true;
                trace!("scheduling `{unit_id:?}`");
                let package_id = unit_id.package_id();
                if observed_packages.contains(package_id) && seen.insert(package_id.to_owned()) {
                    shell::status("Checking", format_package_id(package_id)?)?;
                }
                active_units.insert(unit_id, Default::default());
            }
        }
        if active_units.is_empty() {
            assert!(plan.is_empty(), "{plan:#?}");
            break;
        }

        'units: for (unit_id, state) in active_units.iter_mut() {
            let unit_suggestions = suggestions
                .get(unit_id)
                .expect("finished all active_units without suggestions");
            for path in state.snapshots.keys().chain(unit_suggestions.keys()) {
                let Ok(handle) = same_file::Handle::from_path(path) else {
                    continue;
                };
                match claimed_files.entry(handle) {
                    std::collections::hash_map::Entry::Occupied(entry)
                        if entry.get() != unit_id =>
                    {
                        trace!("deferring `{unit_id:?}` due to contention over {path}");
                        claimed_files.retain(|_k, v| v != unit_id);
                        continue 'units;
                    }
                    std::collections::hash_map::Entry::Occupied(_) => {}
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(unit_id.clone());
                    }
                }
            }
            trace!("fixing `{unit_id:?}` {state:?}");
            state.iterations += 1;
            let _made_changes = fix_suggestions(unit_suggestions, state)?;
        }
    }
    Ok(())
}

/// Packages that Cargo treats as primary for the current invocation.
#[derive(Debug)]
struct PrimaryPackages {
    package_ids: HashSet<String>,
}

impl PrimaryPackages {
    /// Reconstructs Cargo's primary package set from its package-selection flags.
    fn from_metadata(metadata: &Metadata, flags: &CheckFlags) -> CargoResult<Self> {
        let package_ids = match flags.package_selection() {
            PackageSelection::Default => metadata
                .workspace_default_members
                .iter()
                .map(|package_id| package_id.repr.clone())
                .collect(),
            PackageSelection::Workspace { exclude } => {
                let matcher = PackageSpecMatcher::new(exclude)?;
                let mut package_ids = HashSet::new();
                for package in metadata.workspace_packages() {
                    if !matcher.matches(package)? {
                        package_ids.insert(package.id.repr.clone());
                    }
                }
                package_ids
            }
            PackageSelection::Packages(packages) => {
                let matcher = PackageSpecMatcher::new(packages)?;
                let mut package_ids = HashSet::new();
                for package in metadata.workspace_packages() {
                    if matcher.matches(package)? {
                        package_ids.insert(package.id.repr.clone());
                    }
                }
                package_ids
            }
        };
        Ok(Self { package_ids })
    }

    fn contains(&self, package_id: &str) -> bool {
        self.package_ids.contains(package_id)
    }
}

/// Matches Cargo package specifications and package-name glob patterns.
#[derive(Debug)]
struct PackageSpecMatcher {
    specs: Vec<PackageIdSpec>,
    patterns: Vec<glob::Pattern>,
}

impl PackageSpecMatcher {
    fn new(raw_specs: &[String]) -> CargoResult<Self> {
        let mut specs = Vec::new();
        let mut patterns = Vec::new();

        for raw_spec in raw_specs {
            match PackageIdSpec::parse(raw_spec) {
                Ok(spec) => specs.push(spec),
                Err(_) if raw_spec.contains(&['*', '?', '[', ']'][..]) => {
                    let pattern = glob::Pattern::new(raw_spec)
                        .with_context(|| format!("failed to parse package pattern `{raw_spec}`"))?;
                    patterns.push(pattern);
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("failed to parse package specification `{raw_spec}`")
                    });
                }
            }
        }

        Ok(Self { specs, patterns })
    }

    fn matches(&self, package: &cargo_metadata::Package) -> CargoResult<bool> {
        if self
            .patterns
            .iter()
            .any(|pattern| pattern.matches(package.name.as_ref()))
        {
            return Ok(true);
        }

        let package_id = PackageIdSpec::parse(&package.id.repr)
            .with_context(|| format!("failed to parse package ID `{}`", package.id))?;
        Ok(self
            .specs
            .iter()
            .any(|spec| package_id_matches(spec, &package_id)))
    }
}

/// Mirrors Cargo's internal package-ID matching rules.
fn package_id_matches(spec: &PackageIdSpec, package_id: &PackageIdSpec) -> bool {
    spec.name() == package_id.name()
        && spec.partial_version().is_none_or(|version| {
            package_id
                .version()
                .is_some_and(|package_version| version.matches(&package_version))
        })
        && spec.url().is_none_or(|url| package_id.url() == Some(url))
        && spec
            .kind()
            .is_none_or(|kind| package_id.kind() == Some(kind))
}

/// Loads unresolved package metadata once and reuses it for selection and ordering.
fn package_metadata(flags: &CheckFlags) -> CargoResult<Metadata> {
    let mut command = MetadataCommand::new();
    command.no_deps();
    command.other_options(flags.to_metadata_flags());
    let metadata = command.exec().context("failed to run `cargo metadata`")?;
    Ok(metadata)
}

fn finish_unit(
    unit_id: &UnitId,
    active_units: &IndexMap<UnitId, ActiveState>,
    errors: Option<&IndexSet<String>>,
) -> CargoResult<()> {
    trace!("finishing build unit `{unit_id:?}`");
    if let Some(state) = active_units.get(unit_id) {
        for (name, file) in &state.snapshots {
            shell::fixed(name, file.fixes)?;
        }
    }

    for error in errors.into_iter().flatten() {
        shell::print_ansi_stderr(format!("{}\n\n", error.trim_end()).as_bytes())?;
    }

    Ok(())
}

fn check(args: &FixitArgs, lint_cap: &mut bool) -> CargoResult<(Vec<CheckOutput>, Option<i32>)> {
    let mut command = args.to_command();
    command
        .args(["--message-format", "json-diagnostic-rendered-ansi"])
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

fn print_built(args: &FixitArgs, messages: &[CheckOutput]) -> CargoResult<()> {
    if args.verbose == 0 {
        return Ok(());
    }

    for message in messages {
        match message {
            CheckOutput::Message(_) => {}
            CheckOutput::Artifact(a) => {
                if !a.fresh {
                    let pkg_id = format_package_id(&a.build_unit.package_id)?;
                    let name = &a.build_unit.target.name;
                    let kind = &a.build_unit.target.kind;
                    let kind = if 1 < kind.len() {
                        "lib" // HACK: if its multiple, it is only a lib
                    } else {
                        match &kind[0] {
                            TargetKind::Bin => "bin",
                            TargetKind::Test => "test",
                            TargetKind::Bench => "bench",
                            TargetKind::Example => "example",
                            TargetKind::CustomBuild => "custom-build",
                            TargetKind::Lib(_) => "lib",
                        }
                    };
                    shell::status("Checked", format!("{pkg_id} - {name} ({kind})"))?;
                }
            }
        }
    }

    Ok(())
}

/// Applies the original lint cap while preserving existing compiler flags.
fn cap_lints(command: &mut Command) {
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
fn collect_diagnostics(
    messages: impl Iterator<Item = CheckOutput>,
    finished: &BTreeSet<UnitId>,
    primary_packages: &PrimaryPackages,
    active_units: &mut IndexMap<UnitId, ActiveState>,
    max_iterations: usize,
) -> (BuildUnitErrors, BuildUnitSuggestions) {
    let only = HashSet::new();

    let mut suggestions = IndexMap::new();
    let mut errors = IndexMap::new();

    for message in messages {
        let Message {
            build_unit,
            message: MessageDiagnostic { diagnostic, .. },
        } = match message {
            CheckOutput::Message(m) => m,
            CheckOutput::Artifact(a) => {
                let unit_id = UnitId::from_message(&a.build_unit);
                errors.entry(unit_id).or_insert_with(IndexSet::new);
                continue;
            }
        };

        let unit_id = UnitId::from_message(&build_unit);
        if finished.contains(&unit_id) {
            trace!("rejecting build unit `{:?}` already finished", build_unit);
            continue;
        }

        if let Some(state) = active_units.get_mut(&unit_id) {
            if state.iterations >= max_iterations {
                trace!(
                    "rejecting build unit `{:?}` exceeded max iteration count",
                    build_unit
                );
                let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
                if let Some(rendered) = diagnostic.rendered {
                    errors.insert(rendered);
                }
                continue;
            }
        }

        if !primary_packages.contains(&build_unit.package_id) {
            trace!(
                "rejecting build unit `{:?}` not selected by the user",
                build_unit
            );
            let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        }

        let filter = if env::var("__CARGO_FIX_YOLO").is_ok() {
            rustfix::Filter::Everything
        } else {
            rustfix::Filter::MachineApplicableOnly
        };
        let Some(suggestion) = collect_suggestions(&diagnostic, &only, filter) else {
            trace!("rejecting as not a MachineApplicable diagnosis: {diagnostic:?}");
            let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
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
            let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        };

        if !file_names.all(|f| f == file_name) {
            trace!("rejecting as it changes multiple files: {:?}", suggestion);
            let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
            if let Some(rendered) = diagnostic.rendered {
                errors.insert(rendered);
            }
            continue;
        }

        let file_path = Path::new(&file_name);
        // Do not write into registry cache. See rust-lang/cargo#9857.
        if let Ok(home) = env::var("CARGO_HOME") {
            if file_path.starts_with(home) {
                let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
                if let Some(rendered) = diagnostic.rendered {
                    errors.insert(rendered);
                }
                continue;
            }
        }

        if file_path.is_absolute() {
            if let Some(sysroot) = get_sysroot() {
                if file_path.starts_with(sysroot) {
                    let errors = errors.entry(unit_id).or_insert_with(IndexSet::new);
                    if let Some(rendered) = diagnostic.rendered {
                        errors.insert(rendered);
                    }
                    continue;
                }
            }
        }

        let unit_suggestions = suggestions
            .entry(unit_id.clone())
            .or_insert(IndexMap::new());
        unit_suggestions
            .entry(file_name.to_owned())
            .or_insert_with(IndexSet::new)
            .insert((suggestion, diagnostic.rendered));
    }

    (errors, suggestions)
}

#[tracing::instrument(skip_all)]
fn fix_suggestions(
    unit_suggestions: &IndexMap<String, IndexSet<(Suggestion, Option<String>)>>,
    state: &mut ActiveState,
) -> CargoResult<bool> {
    let mut made_changes = false;
    for (file, suggestions) in unit_suggestions {
        let source = match paths::read(file.as_ref()) {
            Ok(s) => s,
            Err(e) => {
                warn!("failed to read `{}`: {}", file, e);
                continue;
            }
        };

        let mut fixed = CodeFix::new(&source);
        let mut num_fixes = 0;

        for (suggestion, _rendered) in suggestions.iter().rev() {
            match fixed.apply(suggestion) {
                Ok(()) => num_fixes += 1,
                Err(rustfix::Error::AlreadyReplaced {
                    is_identical: true, ..
                }) => {}
                Err(e) => {
                    warn!("{e:?}");
                }
            }
        }
        if fixed.modified() {
            let new_source = fixed.finish()?;
            let file_state = state.snapshots.entry(file.clone()).or_insert(File {
                fixes: 0,
                original_source: source,
            });
            paths::write(file, new_source)?;
            made_changes = true;
            file_state.fixes += num_fixes;
        }
    }

    Ok(made_changes)
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct UnitId {
    inner: std::sync::Arc<UnitIdInner>,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct UnitIdInner {
    // HACK: this should also track
    // - test-mode or not
    // - platform
    // - host vs target mode
    package_id: String,
    target_kind: TargetKind,
}

impl UnitId {
    fn from_message(build_unit: &BuildUnit) -> Self {
        // HACK: just collapse all libs to one kind since we can't distinguish them
        let target_kind = build_unit
            .target
            .kind
            .first()
            .expect("build unit targets have at least one kind");
        let target_kind = match target_kind {
            TargetKind::Lib(_) => TargetKind::Lib(CrateType::Lib),
            target_kind => target_kind.clone(),
        };

        Self {
            inner: std::sync::Arc::new(UnitIdInner {
                package_id: build_unit.package_id.clone(),
                target_kind,
            }),
        }
    }

    fn from_metadata(
        package: &cargo_metadata::Package,
        target_kind: &cargo_metadata::TargetKind,
    ) -> Self {
        let target_kind = match target_kind {
            cargo_metadata::TargetKind::Bin => TargetKind::Bin,
            cargo_metadata::TargetKind::Test => TargetKind::Test,
            cargo_metadata::TargetKind::Bench => TargetKind::Bench,
            cargo_metadata::TargetKind::Example => TargetKind::Example,
            cargo_metadata::TargetKind::CustomBuild => TargetKind::CustomBuild,
            // HACK: just collapse all libs to one kind since we can't distinguish them
            cargo_metadata::TargetKind::Lib
            | cargo_metadata::TargetKind::RLib
            | cargo_metadata::TargetKind::DyLib
            | cargo_metadata::TargetKind::CDyLib
            | cargo_metadata::TargetKind::StaticLib
            | cargo_metadata::TargetKind::ProcMacro => TargetKind::Lib(CrateType::Lib),
            target_kind => TargetKind::Lib(CrateType::Other(target_kind.to_string())),
        };

        Self {
            inner: std::sync::Arc::new(UnitIdInner {
                package_id: package.id.repr.clone(),
                target_kind,
            }),
        }
    }

    fn package_id(&self) -> &str {
        &self.inner.package_id
    }

    fn target_kind(&self) -> &TargetKind {
        &self.inner.target_kind
    }
}

#[derive(Debug)]
struct UnitGraph {
    dependencies: BTreeMap<UnitId, BTreeSet<UnitId>>,
    finished: BTreeSet<UnitId>,
}

impl UnitGraph {
    fn flat(metadata: &Metadata) -> Self {
        let mut dependencies = BTreeMap::default();
        for package in &metadata.packages {
            for target in &package.targets {
                for kind in &target.kind {
                    let unit_id = UnitId::from_metadata(package, kind);
                    dependencies.insert(unit_id, Default::default());
                }
            }
        }

        Self {
            dependencies,
            finished: Default::default(),
        }
    }

    fn new(metadata: &Metadata) -> Self {
        let mut dependencies = BTreeMap::default();
        let mut path_to_lib_unit_ids = BTreeMap::default();
        for package in &metadata.packages {
            let mut build_script_unit_id = None;
            let mut lib_unit_ids = BTreeSet::new();
            let mut other_unit_ids = BTreeSet::new();
            for target in &package.targets {
                for kind in &target.kind {
                    let unit_id = UnitId::from_metadata(package, kind);
                    if matches!(unit_id.target_kind(), TargetKind::CustomBuild) {
                        build_script_unit_id = Some(unit_id);
                    } else if matches!(unit_id.target_kind(), TargetKind::Lib(_)) {
                        lib_unit_ids.insert(unit_id);
                    } else {
                        other_unit_ids.insert(unit_id);
                    }
                }
            }

            for unit_id in other_unit_ids {
                let deps = if !lib_unit_ids.is_empty() {
                    lib_unit_ids.clone()
                } else {
                    build_script_unit_id.clone().into_iter().collect()
                };
                dependencies.insert(unit_id, deps);
            }
            if !lib_unit_ids.is_empty() {
                let path_source = manifest_path_to_dep_path(&package.manifest_path);
                path_to_lib_unit_ids.insert(path_source.to_owned(), lib_unit_ids.clone());
                for unit_id in lib_unit_ids {
                    let deps = build_script_unit_id.clone().into_iter().collect();
                    dependencies.insert(unit_id, deps);
                }
            }
            if let Some(unit_id) = build_script_unit_id {
                dependencies.insert(unit_id, Default::default());
            }
        }

        for package in &metadata.packages {
            for dependency in &package.dependencies {
                let Some(dep_path) = &dependency.path else {
                    continue;
                };
                let Some(dep_unit_ids) = path_to_lib_unit_ids.get(dep_path) else {
                    continue;
                };
                for target in &package.targets {
                    for kind in &target.kind {
                        let unit_id = UnitId::from_metadata(package, kind);
                        let applies = match (&unit_id.target_kind(), &dependency.kind) {
                            (TargetKind::CustomBuild, cargo_metadata::DependencyKind::Build) => {
                                true
                            }
                            (TargetKind::Lib(_), cargo_metadata::DependencyKind::Normal) => true,
                            (TargetKind::Lib(_), cargo_metadata::DependencyKind::Development) => {
                                // HACK: should include for unit test variant except that would
                                // cause cycles
                                false
                            }
                            (TargetKind::Bin, cargo_metadata::DependencyKind::Normal) => true,
                            (TargetKind::Bin, cargo_metadata::DependencyKind::Development) => {
                                // HACK: for the unit test variant
                                true
                            }
                            (TargetKind::Test, cargo_metadata::DependencyKind::Normal) => true,
                            (TargetKind::Test, cargo_metadata::DependencyKind::Development) => true,
                            (TargetKind::Bench, cargo_metadata::DependencyKind::Normal) => true,
                            (TargetKind::Bench, cargo_metadata::DependencyKind::Development) => {
                                true
                            }
                            (TargetKind::Example, cargo_metadata::DependencyKind::Normal) => true,
                            (TargetKind::Example, cargo_metadata::DependencyKind::Development) => {
                                true
                            }
                            _ => false,
                        };
                        if applies {
                            dependencies
                                .entry(unit_id)
                                .or_default()
                                .extend(dep_unit_ids.clone());
                        }
                    }
                }
            }
        }

        Self {
            dependencies,
            finished: Default::default(),
        }
    }

    fn is_empty(&self) -> bool {
        self.dependencies.is_empty()
    }

    fn take_ready(&mut self) -> BTreeSet<UnitId> {
        self.dependencies
            .extract_if(.., |_k, v| v.is_empty())
            .map(|(k, _v)| k)
            .collect()
    }

    fn mark_finished(&mut self, finished: BTreeSet<UnitId>) {
        for dependencies in self.dependencies.values_mut() {
            dependencies.retain(|id| !finished.contains(id));
        }
        self.finished.extend(finished);
    }
}

fn manifest_path_to_dep_path(manifest_path: &camino::Utf8Path) -> &camino::Utf8Path {
    if manifest_path.ends_with("Cargo.toml") {
        manifest_path.parent().unwrap()
    } else {
        manifest_path
    }
}

fn is_local(package_id: &str) -> bool {
    package_id.starts_with("path+")
}
