use clap::Parser;

#[derive(Debug, Parser)]
pub struct CheckFlags {
    /// Fix only this package's library
    #[arg(long)]
    lib: bool,

    /// Fix all binaries
    #[arg(long)]
    bins: bool,

    /// Fix only the specified binary
    #[arg(long, value_name = "NAME")]
    bin: Option<String>,

    /// Unstable (nightly-only) flags
    #[arg(short = 'Z', value_name = "FLAG")]
    unstable_flags: Vec<String>,
}

impl CheckFlags {
    pub fn to_flags(&self) -> Vec<String> {
        let mut out = Vec::new();

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

        for i in self.unstable_flags.clone() {
            out.push("-Z".to_owned());
            out.push(i);
        }

        out
    }
}
