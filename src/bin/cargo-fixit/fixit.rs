use cargo_fixit::CargoResult;
use clap::Parser;

#[derive(Debug, Parser)]
pub(crate) struct Fixit {}

impl Fixit {
    pub(crate) fn exec(self) -> CargoResult<()> {
        anyhow::bail!("not implemented");
    }
}
