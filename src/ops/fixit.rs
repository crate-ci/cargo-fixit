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
use clap::Parser;
use indexmap::{IndexMap, IndexSet};
use rustfix::{collect_suggestions, CodeFix, Suggestion};
use tracing::{trace, warn};

use crate::util::cli::PackageSelection;
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

    /// Fix all targets together, risking stale suggestions
    #[arg(long = "Zdangerous-parallel-fixes")]
    dangerous_parallel_fixes: bool,

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

type BuildUnitErrors = IndexMap<BuildUnit, IndexSet<String>>;
type BuildUnitSuggestions =
    IndexMap<BuildUnit, IndexMap<String, IndexSet<(Suggestion, Option<String>)>>>;

#[tracing::instrument(skip_all)]
fn exec(args: FixitArgs) -> CargoResult<()> {
    args.color.write_global();

    args.vcs_opts.valid_vcs()?;

    let mut active_targets = IndexMap::new();
    match fix(&args, &mut active_targets) {
        Ok(()) => Ok(()),
        Err(error) => {
            for (file, original) in active_targets.values().flat_map(|files| files.iter()) {
                paths::write(file, &original.original_source)?;
            }
            Err(error)
        }
    }
}

fn fix(
    args: &FixitArgs,
    active_targets: &mut IndexMap<BuildUnit, IndexMap<String, File>>,
) -> CargoResult<()> {
    let max_iterations: usize = env::var("CARGO_FIX_MAX_RETRIES")
        .ok()
        .and_then(|i| i.parse().ok())
        .unwrap_or(4);
    let mut iteration = 0;
    let mut lint_cap = false;

    let mut last_errors = IndexMap::new();
    let mut claimed_files: HashMap<same_file::Handle, BuildUnit> = HashMap::new();
    let mut package_metadata_cache = None;
    let mut primary_packages_cache = None;
    let mut package_graph_cache: Option<Option<PackageGraph>> = None;
    let mut seen = HashSet::new();

    loop {
        trace!("iteration={iteration}");
        trace!("active_targets={active_targets:?}");
        let (messages, exit_code) = check(args, &mut lint_cap)?;

        if !args.broken_code && exit_code != Some(0) {
            let mut out = String::new();

            if !active_targets.is_empty() {
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
                ) in active_targets.values().flat_map(|files| files.iter())
                {
                    out.push_str(&format!("  * {file}\n"));
                    shell::note(format!("reverting `{file}` to its original state"))?;
                    paths::write(file, original_source)?;
                }
                active_targets.clear();
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
        if build_unit_map.values().any(|file_map| !file_map.is_empty()) {
            let primary_packages = if let Some(primary_packages) = &primary_packages_cache {
                primary_packages
            } else {
                let metadata = package_metadata(&mut package_metadata_cache, &args.check_flags)?;
                primary_packages_cache
                    .insert(PrimaryPackages::from_metadata(metadata, &args.check_flags)?)
            };
            retain_primary_fixes(primary_packages, &mut errors, &mut build_unit_map);
        }

        if iteration >= max_iterations {
            if active_targets.is_empty() {
                break;
            }
            let targets: Vec<_> = active_targets.keys().cloned().collect();
            for target in targets {
                if let Some(file_map) = build_unit_map.get(&target) {
                    let target_errors = errors.entry(target.clone()).or_default();
                    target_errors.extend(
                        file_map
                            .values()
                            .flatten()
                            .filter_map(|(_, diagnostic)| diagnostic.clone()),
                    );
                }
                finish_target(target, active_targets, &mut errors, &mut seen)?;
            }
            claimed_files.clear();
            iteration = 0;
        }

        let mut finalized_targets = false;
        if !active_targets.is_empty()
            && active_targets
                .keys()
                .all(|target| build_unit_map.get(target).is_none_or(IndexMap::is_empty))
        {
            let targets: Vec<_> = active_targets.keys().cloned().collect();
            for target in targets {
                build_unit_map.shift_remove(&target);
                finish_target(target, active_targets, &mut errors, &mut seen)?;
            }
            debug_assert!(active_targets.is_empty());
            claimed_files.clear();
            iteration = 0;
            finalized_targets = true;
        }

        let mut made_changes = false;
        // Admit build units from one compiler snapshot only when their packages are independent.
        // Once a batch is active, recheck and finish it before considering additional units.
        let continuing_batch = !active_targets.is_empty();

        for (build_unit, file_map) in build_unit_map {
            if seen.contains(&build_unit) {
                continue;
            }

            let build_unit_errors = errors
                .entry(build_unit.clone())
                .or_insert_with(IndexSet::new);

            if active_targets.is_empty() && file_map.is_empty() {
                if finalized_targets && build_unit_errors.is_empty() {
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
            } else if !file_map.is_empty() {
                let was_active = active_targets.contains_key(&build_unit);
                if continuing_batch && !was_active {
                    continue;
                }

                if !args.dangerous_parallel_fixes && !was_active && !active_targets.is_empty() {
                    if active_targets
                        .keys()
                        .any(|active| active.package_id == build_unit.package_id)
                    {
                        continue;
                    }

                    if package_graph_cache.is_none() {
                        let metadata =
                            package_metadata(&mut package_metadata_cache, &args.check_flags)?;
                        package_graph_cache = Some(PackageGraph::load(metadata, &args.check_flags));
                    }
                    let Some(Some(graph)) = package_graph_cache.as_mut() else {
                        continue;
                    };

                    let mut independent = true;
                    for active in active_targets.keys() {
                        if !graph
                            .packages_are_independent(&active.package_id, &build_unit.package_id)
                        {
                            independent = false;
                            break;
                        }
                    }
                    if !independent {
                        continue;
                    }
                }

                let handles = file_map
                    .keys()
                    .map(same_file::Handle::from_path)
                    .collect::<Result<Vec<_>, _>>()
                    .ok();
                let serialize_target = handles.is_none();
                if serialize_target && !was_active && !active_targets.is_empty() {
                    continue;
                }
                if handles.as_ref().is_some_and(|handles| {
                    handles.iter().any(|handle| {
                        claimed_files
                            .get(handle)
                            .is_some_and(|owner| owner != &build_unit)
                    })
                }) {
                    continue;
                }

                let target_files = active_targets.entry(build_unit.clone()).or_default();
                let changed = fix_errors(target_files, file_map, build_unit_errors)?;
                if !changed && !was_active {
                    active_targets.shift_remove(&build_unit);
                }
                if changed {
                    if let Some(handles) = handles {
                        for handle in handles {
                            claimed_files.entry(handle).or_insert(build_unit.clone());
                        }
                    }
                    made_changes = true;
                    if serialize_target {
                        break;
                    }
                }
            }
        }

        trace!("made_changes={made_changes:?}");
        trace!("active_targets={active_targets:?}");

        last_errors = errors;
        iteration += 1;

        if !made_changes {
            if active_targets.is_empty() {
                break;
            }
            let targets: Vec<_> = active_targets.keys().cloned().collect();
            for target in targets {
                finish_target(target, active_targets, &mut last_errors, &mut seen)?;
            }
            claimed_files.clear();
            iteration = 0;
            continue;
        }
    }

    for files in active_targets.values() {
        for (name, file) in files {
            shell::fixed(name, file.fixes)?;
        }
    }

    for e in last_errors.iter().flat_map(|(_, e)| e) {
        shell::print_ansi_stderr(format!("{}\n\n", e.trim_end()).as_bytes())?;
    }

    active_targets.clear();
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
fn package_metadata<'a>(
    cache: &'a mut Option<Metadata>,
    flags: &CheckFlags,
) -> CargoResult<&'a Metadata> {
    match cache {
        Some(metadata) => Ok(metadata),
        cache @ None => {
            let mut command = MetadataCommand::new();
            command.no_deps();
            command.other_options(flags.to_metadata_flags());
            let metadata = command.exec().context("failed to run `cargo metadata`")?;
            Ok(cache.insert(metadata))
        }
    }
}

/// Discards dependency suggestions while preserving their diagnostics for display.
fn retain_primary_fixes(
    primary_packages: &PrimaryPackages,
    errors: &mut BuildUnitErrors,
    build_unit_map: &mut BuildUnitSuggestions,
) {
    for (build_unit, file_map) in build_unit_map {
        if file_map.is_empty() || primary_packages.contains(&build_unit.package_id) {
            continue;
        }

        let build_unit_errors = errors.entry(build_unit.clone()).or_default();
        build_unit_errors.extend(
            file_map
                .values()
                .flatten()
                .filter_map(|(_, diagnostic)| diagnostic.clone()),
        );
        file_map.clear();
    }
}

/// Package dependencies used to batch only transitively unrelated packages.
#[derive(Debug)]
struct PackageGraph {
    dependencies: HashMap<String, Vec<String>>,
    reachable: HashMap<String, HashSet<String>>,
}

impl PackageGraph {
    /// Loads the package graph, returning `None` when batching must remain serial.
    fn load(metadata: &Metadata, flags: &CheckFlags) -> Option<Self> {
        // A script is identified by its manifest path, not its containing directory.
        if metadata
            .packages
            .iter()
            .any(|package| package.manifest_path.file_name() != Some("Cargo.toml"))
        {
            return Self::load_resolved(flags);
        }

        let package_ids_by_path: HashMap<_, _> = metadata
            .packages
            .iter()
            .filter_map(|package| {
                package
                    .manifest_path
                    .parent()
                    .map(|path| (path, package.id.repr.as_str()))
            })
            .collect();
        if package_ids_by_path.len() != metadata.packages.len() {
            return Self::load_resolved(flags);
        }

        let package_names: HashSet<_> = metadata
            .packages
            .iter()
            .map(|package| package.name.as_ref())
            .collect();
        let mut dependencies = HashMap::with_capacity(metadata.packages.len());
        for package in &metadata.packages {
            let mut package_dependencies = Vec::new();
            for dependency in &package.dependencies {
                if let Some(path) = &dependency.path {
                    let Some(dependency_id) = package_ids_by_path.get(path.as_path()) else {
                        return Self::load_resolved(flags);
                    };
                    package_dependencies.push((*dependency_id).to_owned());
                } else if package_names.contains(dependency.name.as_str()) {
                    return Self::load_resolved(flags);
                }
            }
            dependencies.insert(package.id.repr.clone(), package_dependencies);
        }

        Some(Self {
            dependencies,
            reachable: HashMap::new(),
        })
    }

    /// Resolves external packages when workspace metadata cannot prove independence.
    fn load_resolved(flags: &CheckFlags) -> Option<Self> {
        let mut command = MetadataCommand::new();
        command.other_options(flags.to_metadata_flags());

        let metadata = match command.exec() {
            Ok(metadata) => metadata,
            Err(error) => {
                warn!("failed to run `cargo metadata`: {error}");
                return None;
            }
        };
        let Some(resolve) = metadata.resolve else {
            warn!("`cargo metadata` did not return a dependency graph");
            return None;
        };
        let dependencies = resolve
            .nodes
            .into_iter()
            .map(|node| {
                (
                    node.id.repr,
                    node.dependencies
                        .into_iter()
                        .map(|dependency| dependency.repr)
                        .collect(),
                )
            })
            .collect();

        Some(Self {
            dependencies,
            reachable: HashMap::new(),
        })
    }

    /// Returns whether both packages are known and transitively unrelated.
    fn packages_are_independent(&mut self, left: &str, right: &str) -> bool {
        left != right && !self.depends_on(left, right) && !self.depends_on(right, left)
    }

    /// Returns whether `package` transitively depends on `target`.
    fn depends_on(&mut self, package: &str, target: &str) -> bool {
        if !self.reachable.contains_key(package) {
            let Some(reachable) = self.collect_reachable(package) else {
                return true;
            };
            self.reachable.insert(package.to_owned(), reachable);
        }

        self.reachable
            .get(package)
            .is_none_or(|reachable| reachable.contains(target))
    }

    /// Collects the packages transitively reachable from `root`.
    fn collect_reachable(&self, root: &str) -> Option<HashSet<String>> {
        let mut reachable = HashSet::new();
        let mut pending = vec![root];

        while let Some(package) = pending.pop() {
            if !reachable.insert(package.to_owned()) {
                continue;
            }
            let dependencies = self.dependencies.get(package)?;
            pending.extend(dependencies.iter().map(String::as_str));
        }

        reachable.remove(root);
        Some(reachable)
    }
}

/// Marks a target complete after reporting its fixes and remaining diagnostics.
fn finish_target(
    target: BuildUnit,
    active_targets: &mut IndexMap<BuildUnit, IndexMap<String, File>>,
    errors: &mut BuildUnitErrors,
    seen: &mut HashSet<BuildUnit>,
) -> CargoResult<()> {
    if seen
        .iter()
        .all(|build_unit| build_unit.package_id != target.package_id)
    {
        shell::status("Checking", format_package_id(&target.package_id)?)?;
    }

    if let Some(files) = active_targets.get(&target) {
        for (name, file) in files {
            shell::fixed(name, file.fixes)?;
        }
    }

    for error in errors.get(&target).into_iter().flatten() {
        shell::print_ansi_stderr(format!("{}\n\n", error.trim_end()).as_bytes())?;
    }

    active_targets.shift_remove(&target);
    errors.shift_remove(&target);
    seen.insert(target);
    Ok(())
}

fn check(args: &FixitArgs, lint_cap: &mut bool) -> CargoResult<(Vec<CheckOutput>, Option<i32>)> {
    let cmd = if args.clippy { "clippy" } else { "check" };
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
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
fn collect_errors(
    messages: impl Iterator<Item = CheckOutput>,
    seen: &HashSet<BuildUnit>,
) -> (BuildUnitErrors, BuildUnitSuggestions) {
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
