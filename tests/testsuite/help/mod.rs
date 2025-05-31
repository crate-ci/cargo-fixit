use snapbox::cmd::cargo_bin;

#[test]
fn help() {
    snapbox::cmd::Command::new(cargo_bin!("cargo-fixit"))
        .args(["fixit", "--help"])
        .assert()
        .success()
        .stdout_eq(snapbox::file!["stdout.term.svg"])
        .stderr_eq(snapbox::str![""]);
}
