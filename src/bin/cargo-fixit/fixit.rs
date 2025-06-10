use cargo_fixit::CargoResult;
use clap::Parser;

#[derive(Debug, Parser)]
pub(crate) struct FixitArgs {
    /// Unstable (nightly-only) flags
    #[arg(short = 'Z', value_name = "FLAG")]
    unstable_flags: Vec<String>,
}

impl FixitArgs {
    pub(crate) fn exec(self) -> CargoResult<()> {
        anyhow::bail!("not implemented");
    }
}
