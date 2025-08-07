use std::{path::PathBuf, process::Command, sync::OnceLock};

static SYSROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

pub(crate) fn get_sysroot() -> &'static Option<PathBuf> {
    SYSROOT.get_or_init(|| {
        Command::new("rustc")
            .arg("--print=sysroot")
            .output()
            .map(|x| String::from_utf8_lossy(&x.stdout).trim().to_owned())
            .map(PathBuf::from)
            .ok()
    })
}
