use clap::Parser;

#[derive(Debug, Parser)]
pub struct CheckFlags {
    /// Package(s) to fix
    #[arg(long, value_name = "SPEC", help_heading = "Package Selection")]
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
}

impl CheckFlags {
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

        out
    }
}
