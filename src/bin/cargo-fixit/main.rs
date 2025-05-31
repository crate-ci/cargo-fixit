use clap::Parser as _;

mod cli;

fn main() {
    let cli::Command::Fixit(_args) = cli::Command::parse();

    unimplemented!();
}
