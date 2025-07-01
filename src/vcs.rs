use clap::Parser;

#[derive(Parser, Debug)]
pub struct VCSopts {
    /// Fix code even if a VCS was not detected
    #[arg(long)]
    pub allow_no_vcs: bool,
}
