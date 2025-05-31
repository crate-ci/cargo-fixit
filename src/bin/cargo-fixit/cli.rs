use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "cargo")]
#[command(bin_name = "cargo")]
#[command(styles = clap_cargo::style::CLAP_STYLING)]
pub(crate) enum Command {
    #[command(about, author, version)]
    Fixit(Fixit),
}

#[derive(Debug, clap::Parser)]
pub(crate) struct Fixit {
    // Fixit flags
}

#[test]
fn verify_app() {
    use clap::CommandFactory;
    Command::command().debug_assert();
}
