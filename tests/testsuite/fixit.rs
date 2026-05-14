use cargo_test_support::cargo_test;
use cargo_test_support::{basic_manifest, compare::assert_ui, paths, project};
use snapbox::str;

use crate::fix::FixitProject;

#[cargo_test]
fn basic() {
    let p = project()
        .file(
            "src/lib.rs",
            r#"
            pub fn a() {
                let mut b = 10;
                let _ = b;
            }
            "#,
        )
        .build();

    p.cargo_("fixit --allow-no-vcs")
        .with_status(0)
        .with_stderr_data(str![[r#"
[CHECKING] foo v0.0.1
[FIXED] src/lib.rs (1 fix)

"#]])
        .run();
    assert_ui().eq(
        p.read_file("src/lib.rs"),
        str![[r#"

            pub fn a() {
                let b = 10;
                let _ = b;
            }
            
"#]],
    );
}

#[cargo_test]
fn uses_runtime_cargo_env_var() {
    let fake_cargo = project()
        .at(paths::global_root().join("fake-cargo-for-fixit"))
        .file("Cargo.toml", &basic_manifest("fake-cargo-for-fixit", "1.0.0"))
        .file(
            "src/main.rs",
            r#"
            fn main() {
                let marker = std::env::var_os("CARGO_FIXIT_FAKE_CARGO_MARKER").unwrap();
                let args = std::env::args().skip(1).collect::<Vec<_>>().join("\n");
                std::fs::write(marker, args).unwrap();
            }
            "#,
        )
        .build();
    fake_cargo.cargo_("build").run();

    let marker = paths::root().join("fake-cargo-for-fixit-called");
    let p = project().file("src/lib.rs", "pub fn a() {}").build();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cargo-fixit"))
        .args(["fixit", "--allow-no-vcs"])
        .current_dir(p.root())
        .env("CARGO", fake_cargo.bin("fake-cargo-for-fixit"))
        .env("CARGO_FIXIT_FAKE_CARGO_MARKER", &marker)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        std::fs::read_to_string(marker).unwrap(),
        "check\n--message-format\njson-diagnostic-rendered-ansi"
    );
}

#[cargo_test]
fn fixable_and_unfixable() {
    let p = project()
        .file(
            "src/lib.rs",
            r#"
            pub fn a() {
                let mut b = 10;
                let _ = b;

                let mut c = 10;
                let _ = c;
                c = 1;
            }
            "#,
        )
        .build();

    p.cargo_("fixit --allow-no-vcs")
        .with_status(0)
        .with_stderr_data(str![[r#"
[CHECKING] foo v0.0.1
[FIXED] src/lib.rs (1 fix)
[WARNING] value assigned to `c` is never read
 --> src/lib.rs:8:17
  |
8 |                 c = 1;
  |                 ^^^^^
  |
  = [HELP] maybe it is overwritten before being read?
  = [NOTE] `#[warn(unused_assignments)]` (part of `#[warn(unused)]`) on by default


"#]])
        .run();
    assert_ui().eq(
        p.read_file("src/lib.rs"),
        str![[r#"

            pub fn a() {
                let b = 10;
                let _ = b;

                let mut c = 10;
                let _ = c;
                c = 1;
            }
            
"#]],
    );
}

#[cargo_test]
fn dependency_order() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
            [workspace]
            members = [ "a", "b", "c", "d" ]
            "#,
        )
        .file(
            "a/Cargo.toml",
            r#"
                [package]
                name = "a"
                version = "0.1.0"
                edition = "2024"

                [dependencies]
                b = { path = "../b" }
                c = { path = "../c" }
            "#,
        )
        .file("a/src/lib.rs", "use std as foo;")
        .file(
            "b/Cargo.toml",
            r#"
                [package]
                name = "b"
                version = "0.1.0"
                edition = "2024"

                [dependencies]
                d = { path = "../d" }
            "#,
        )
        .file("b/src/lib.rs", "use std as foo;")
        .file("c/Cargo.toml", &basic_manifest("c", "0.1.0"))
        .file("c/src/lib.rs", "use std as foo;")
        .file("d/Cargo.toml", &basic_manifest("d", "0.1.0"))
        .file("d/src/lib.rs", "use std as foo;")
        .build();

    p.cargo_("build").with_status(0).run();
    p.cargo_("fixit --allow-no-vcs")
        .with_status(0)
        .with_stderr_data(str![[r#"
...
[FIXED] d/src/lib.rs (1 fix)
...
[FIXED] b/src/lib.rs (1 fix)
...
[FIXED] a/src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn build_unit_order() {
    let p = project()
        .file("Cargo.toml", &basic_manifest("foo", "0.1.0"))
        .file("build.rs", "fn main(){ let mut a = 1; let _ = a; }")
        .file("src/lib.rs", "fn _a(){ let mut a = 1; let _ = a; }")
        .file("src/main.rs", "fn main(){ let mut a = 1; let _ = a; }")
        .build();

    p.cargo_("fixit --allow-no-vcs")
        .with_status(0)
        .with_stderr_data(str![[r#"
[CHECKING] foo v0.1.0
[FIXED] build.rs (1 fix)
[FIXED] src/lib.rs (1 fix)
[FIXED] src/main.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn print_errors_after_fixed() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
            [workspace]
            members = [ "a", "b" ]
            "#,
        )
        .file(
            "a/Cargo.toml",
            r#"
                [package]
                name = "a"
                version = "0.1.0"
                edition = "2024"

                [dependencies]
                b = { path = "../b" }
            "#,
        )
        .file("a/src/lib.rs", "use std as foo; fn bar() {}")
        .file("b/Cargo.toml", &basic_manifest("b", "0.1.0"))
        .file("b/src/lib.rs", "use std as foo; fn bar() {}")
        .build();

    p.cargo_("fixit --allow-no-vcs")
        .with_status(0)
        .with_stderr_data(str![[r#"
[CHECKING] b v0.1.0
[FIXED] b/src/lib.rs (1 fix)
[WARNING] function `bar` is never used
 --> b/src/lib.rs:1:5
  |
1 |  fn bar() {}
  |     ^^^
  |
  = [NOTE] `#[warn(dead_code)]` [..]on by default

[CHECKING] a v0.1.0
[FIXED] a/src/lib.rs (1 fix)
[WARNING] function `bar` is never used
 --> a/src/lib.rs:1:5
  |
1 |  fn bar() {}
  |     ^^^
  |
  = [NOTE] `#[warn(dead_code)]` [..]on by default


"#]])
        .run();
}
