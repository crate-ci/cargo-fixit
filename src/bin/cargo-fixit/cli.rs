use cargo_fixit::CargoResult;
use clap::Parser;

#[derive(Debug, Parser)]
#[command(bin_name = "cargo")]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
pub(crate) enum Command {
    #[command(subcommand)]
    Plumbing(crate::fixit::Plumbing),
}

impl Command {
    pub(crate) fn exec(self) -> CargoResult<()> {
        match self {
            Self::Plumbing(fixit) => fixit.exec(),
        }
    }
}

#[test]
fn verify_app() {
    use clap::CommandFactory;
    Command::command().debug_assert()
}
