use clap::Parser;

use crate::CargoResult;

#[derive(Debug, Parser)]
pub struct CheckFlags {
    /// Package(s) to fix
    #[arg(short, long, value_name = "SPEC", help_heading = "Package Selection")]
    package: Vec<String>,

    /// Fix all packages in the workspace
    #[arg(long, help_heading = "Package Selection")]
    workspace: bool,

    /// Exclude packages from the fixes
    #[arg(long, value_name = "SPEC", help_heading = "Package Selection")]
    exclude: Vec<String>,

    /// Alias for --workspace (deprecated)
    #[arg(long, help_heading = "Package Selection")]
    all: bool,

    /// Fix only this package's library
    #[arg(long, help_heading = "Target Selection")]
    lib: bool,

    /// Fix all binaries
    #[arg(long, help_heading = "Target Selection")]
    bins: bool,

    /// Fix only the specified binary
    #[arg(long, value_name = "NAME", help_heading = "Target Selection")]
    bin: Option<String>,

    /// Fix all examples
    #[arg(long, help_heading = "Target Selection")]
    examples: bool,

    /// Fix only the specified binary
    #[arg(long, value_name = "NAME", help_heading = "Target Selection")]
    example: Option<String>,

    /// Fix all tests
    #[arg(long, help_heading = "Target Selection")]
    tests: bool,

    /// Fix only the specified test
    #[arg(long, value_name = "NAME", help_heading = "Target Selection")]
    test: Option<String>,

    /// Fix all benches
    #[arg(long, help_heading = "Target Selection")]
    benches: bool,

    /// Fix only the specified bench
    #[arg(long, value_name = "NAME", help_heading = "Target Selection")]
    bench: Option<String>,

    /// Fix all targets
    #[arg(long, help_heading = "Target Selection")]
    all_targets: bool,

    /// Space or comma separated list of features to activate
    #[arg(
        short = 'F',
        long,
        value_name = "FEATURES",
        help_heading = "Feature Selection"
    )]
    features: Vec<String>,

    /// Activate all available features
    #[arg(long, help_heading = "Feature Selection")]
    all_features: bool,

    /// Do not activate the `default` feature
    #[arg(long, help_heading = "Feature Selection")]
    no_default_features: bool,

    /// Unstable (nightly-only) flags
    #[arg(short = 'Z', value_name = "FLAG")]
    unstable_flags: Vec<String>,

    /// Number of parallel jobs, defaults to # of CPUs.
    #[arg(long, value_name = "N", help_heading = "Compilation Options")]
    jobs: Option<usize>,

    /// Fix artifacts in release mode, with optimizations
    #[arg(long, help_heading = "Compilation Options")]
    release: bool,

    /// Build artifacts with the specified profile
    #[arg(
        long,
        value_name = "PROFILE-NAME",
        help_heading = "Compilation Options"
    )]
    profile: Option<String>,

    /// Fix for the target triple
    #[arg(long, value_name = "TRIPLE", help_heading = "Compilation Options")]
    target: Vec<String>,

    /// Directory for all generated artifacts
    #[arg(long, value_name = "DIRECTORY", help_heading = "Compilation Options")]
    target_dir: Option<String>,

    /// Path to Cargo.toml
    #[arg(long, value_name = "PATH", help_heading = "Manifest Options")]
    manifest_path: Option<String>,

    /// Path to Cargo.lock (unstable)
    #[arg(long, value_name = "PATH", help_heading = "Manifest Options")]
    lockfile_path: Option<String>,

    /// Ignore `rust-version` specification in packages
    #[arg(long, help_heading = "Manifest Options")]
    ignore_rust_version: bool,

    /// Assert that `Cargo.lock` will remain unchanged
    #[arg(long, help_heading = "Manifest Options")]
    locked: bool,

    /// Run without accessing the network
    #[arg(long, help_heading = "Manifest Options")]
    offline: bool,

    /// Equivalent to specifying both --locked and --offline
    #[arg(long, help_heading = "Manifest Options")]
    frozen: bool,
}

/// Package selectors that determine which workspace packages Cargo treats as primary.
#[derive(Debug)]
pub(crate) enum PackageSelection<'a> {
    Default,
    Workspace { exclude: &'a [String] },
    Packages(&'a [String]),
}

impl CheckFlags {
    pub(crate) fn package_selection(&self) -> PackageSelection<'_> {
        if self.workspace || self.all {
            PackageSelection::Workspace {
                exclude: &self.exclude,
            }
        } else if self.package.is_empty() {
            debug_assert!(self.exclude.is_empty());
            PackageSelection::Default
        } else {
            debug_assert!(self.exclude.is_empty());
            PackageSelection::Packages(&self.package)
        }
    }

    /// Whether one of this package's targets is explicitly selected for fixing.
    pub(crate) fn selects_package_targets(
        &self,
        package: &cargo_metadata::Package,
    ) -> CargoResult<bool> {
        if self.all_targets || !self.has_target_selection() {
            return Ok(true);
        }

        for target in &package.targets {
            if self.selects_target(target)? {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn has_target_selection(&self) -> bool {
        self.lib
            || self.bins
            || self.bin.is_some()
            || self.examples
            || self.example.is_some()
            || self.tests
            || self.test.is_some()
            || self.benches
            || self.bench.is_some()
    }

    fn selects_target(&self, target: &cargo_metadata::Target) -> CargoResult<bool> {
        let is_lib = target.kind.iter().any(|kind| {
            matches!(
                kind,
                cargo_metadata::TargetKind::Lib
                    | cargo_metadata::TargetKind::RLib
                    | cargo_metadata::TargetKind::DyLib
                    | cargo_metadata::TargetKind::CDyLib
                    | cargo_metadata::TargetKind::StaticLib
                    | cargo_metadata::TargetKind::ProcMacro
            )
        });

        if self.lib && is_lib {
            return Ok(true);
        }
        if self.bins && target.is_bin() {
            return Ok(true);
        }
        if target.is_bin() && matches_target_name(self.bin.as_deref(), &target.name)? {
            return Ok(true);
        }
        if self.examples && target.is_example() {
            return Ok(true);
        }
        if target.is_example() && matches_target_name(self.example.as_deref(), &target.name)? {
            return Ok(true);
        }
        if self.tests && (target.is_test() || target.test) {
            return Ok(true);
        }
        if target.is_test() && matches_target_name(self.test.as_deref(), &target.name)? {
            return Ok(true);
        }
        if self.benches && target.is_bench() {
            // HACK: no `target.bench` in `cargo metadata` output
            return Ok(true);
        }
        if target.is_bench() && matches_target_name(self.bench.as_deref(), &target.name)? {
            return Ok(true);
        }

        Ok(false)
    }

    pub fn to_flags(&self) -> Vec<String> {
        let mut out = Vec::new();

        for spec in self.package.clone() {
            out.push("--package".to_owned());
            out.push(spec);
        }
        if self.workspace {
            out.push("--workspace".to_owned());
        }
        for spec in self.exclude.clone() {
            out.push("--exclude".to_owned());
            out.push(spec);
        }
        if self.all {
            out.push("--all".to_owned());
        }

        if self.lib {
            out.push("--lib".to_owned());
        }

        if self.bins {
            out.push("--bins".to_owned());
        }
        if let Some(b) = self.bin.clone() {
            out.push("--bin".to_owned());
            out.push(b);
        }

        if self.examples {
            out.push("--examples".to_owned());
        }
        if let Some(b) = self.example.clone() {
            out.push("--example".to_owned());
            out.push(b);
        }

        if self.tests {
            out.push("--tests".to_owned());
        }
        if let Some(b) = self.test.clone() {
            out.push("--test".to_owned());
            out.push(b);
        }

        if self.benches {
            out.push("--benches".to_owned());
        }
        if let Some(b) = self.bench.clone() {
            out.push("--bench".to_owned());
            out.push(b);
        }

        if self.all_targets {
            out.push("--all-targets".to_owned());
        }

        for i in self.features.clone() {
            out.push("--features".to_owned());
            out.push(i);
        }
        if self.all_features {
            out.push("--all-features".to_owned());
        }
        if self.no_default_features {
            out.push("--no-default-features".to_owned());
        }

        for i in self.unstable_flags.clone() {
            out.push("-Z".to_owned());
            out.push(i);
        }

        if let Some(b) = self.jobs {
            out.push("--jobs".to_owned());
            out.push(b.to_string());
        }
        if self.release {
            out.push("--release".to_owned());
        }
        if let Some(b) = self.profile.clone() {
            out.push("--profile".to_owned());
            out.push(b);
        }

        for spec in self.target.clone() {
            out.push("--target".to_owned());
            out.push(spec);
        }
        if let Some(b) = self.target_dir.clone() {
            out.push("--target-dir".to_owned());
            out.push(b);
        }

        if let Some(b) = self.manifest_path.clone() {
            out.push("--manifest-path".to_owned());
            out.push(b);
        }
        if let Some(b) = self.lockfile_path.clone() {
            out.push("--lockfile-path".to_owned());
            out.push(b);
        }
        if self.ignore_rust_version {
            out.push("--ignore-rust-version".to_owned());
        }
        if self.locked {
            out.push("--locked".to_owned());
        }
        if self.offline {
            out.push("--offline".to_owned());
        }
        if self.frozen {
            out.push("--frozen".to_owned());
        }
        out
    }

    /// Returns flags that can affect dependency resolution.
    ///
    /// Package and target filters are omitted so the resulting graph stays conservative.
    pub(crate) fn to_metadata_flags(&self) -> Vec<String> {
        let mut out = Vec::new();

        for feature in &self.features {
            out.push("--features".to_owned());
            out.push(feature.clone());
        }
        if self.all_features {
            out.push("--all-features".to_owned());
        }
        if self.no_default_features {
            out.push("--no-default-features".to_owned());
        }

        for flag in &self.unstable_flags {
            out.push("-Z".to_owned());
            out.push(flag.clone());
        }

        if let Some(path) = &self.manifest_path {
            out.push("--manifest-path".to_owned());
            out.push(path.clone());
        }
        if let Some(path) = &self.lockfile_path {
            out.push("--lockfile-path".to_owned());
            out.push(path.clone());
        }
        if self.locked {
            out.push("--locked".to_owned());
        }
        if self.offline {
            out.push("--offline".to_owned());
        }
        if self.frozen {
            out.push("--frozen".to_owned());
        }

        out
    }
}

fn matches_target_name(requested: Option<&str>, actual: &str) -> CargoResult<bool> {
    requested
        .map(|pattern| {
            glob::Pattern::new(pattern)
                .map(|pattern| pattern.matches(actual))
                .map_err(Into::into)
        })
        .unwrap_or(Ok(false))
}
