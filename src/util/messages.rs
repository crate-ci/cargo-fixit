pub fn gen_please_report_this_bug_text(clippy: bool) -> String {
    format!(
        "This likely indicates a bug in either rustc or cargo itself,\n\
     and we would appreciate a bug report! You're likely to see\n\
     a number of compiler warnings after this message which cargo\n\
     attempted to fix but failed. If you could open an issue at\n\
     {}\n\
     quoting the full output of this command we'd be very appreciative!\n\
     Note that you may be able to make some more progress in the near-term\n\
     fixing code with the `--broken-code` flag\n\n\
     ",
        if clippy {
            "https://github.com/rust-lang/rust-clippy/issues"
        } else {
            "https://github.com/rust-lang/rust/issues"
        }
    )
}
