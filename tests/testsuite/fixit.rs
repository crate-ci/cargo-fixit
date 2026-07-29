use cargo_test_support::cargo_test;
use cargo_test_support::{basic_manifest, compare::assert_ui, project, Project};
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
fn preserves_workspace_fingerprints_without_denied_warnings() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"
                [package]
                name = "foo"
                version = "0.1.0"
                edition = "2021"

                [dependencies]
                cached-dependency = { path = "cached-dependency" }
            "#,
        )
        .file("src/lib.rs", "pub fn foo() { cached_dependency::foo(); }")
        .file(
            "cached-dependency/Cargo.toml",
            &basic_manifest("cached-dependency", "0.1.0"),
        )
        .file("cached-dependency/src/lib.rs", "pub fn foo() {}")
        .build();

    p.cargo_("check").run();

    let fingerprints_before = package_fingerprints(&p, &["foo-", "cached-dependency-"]);
    assert_eq!(fingerprints_before.len(), 2);

    p.cargo_("fixit --allow-no-vcs").run();

    assert_eq!(
        package_fingerprints(&p, &["foo-", "cached-dependency-"]),
        fingerprints_before
    );
}

#[cargo_test]
fn clippy_preserves_workspace_fingerprints_without_denied_warnings() {
    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = ['app', 'cached-dependency']\nresolver = '2'\n",
        )
        .file(
            "app/Cargo.toml",
            &format!(
                "{}\n[dependencies]\ncached-dependency = {{ path = '../cached-dependency' }}\n",
                basic_manifest("app", "0.1.0")
            ),
        )
        .file(
            "app/src/lib.rs",
            "pub fn app() { cached_dependency::foo(); }\n",
        )
        .file(
            "cached-dependency/Cargo.toml",
            &basic_manifest("cached-dependency", "0.1.0"),
        )
        .file("cached-dependency/src/lib.rs", "pub fn foo() {}\n")
        .build();

    p.cargo_("clippy --workspace").run();

    let fingerprints_before = package_fingerprints(&p, &["app-", "cached-dependency-"]);
    assert_eq!(fingerprints_before.len(), 2);

    p.cargo_("fixit --clippy --workspace --allow-no-vcs").run();

    assert_eq!(
        package_fingerprints(&p, &["app-", "cached-dependency-"]),
        fingerprints_before
    );
}

#[cargo_test]
fn clippy_fixes_denied_warnings() {
    let p = project()
        .file(
            "src/lib.rs",
            "#![deny(warnings)]\npub fn a() { let mut value = 1; let _ = value; }\n",
        )
        .build();

    p.cargo_("fixit --clippy --allow-no-vcs")
        .with_status(0)
        .run();

    assert!(!p.read_file("src/lib.rs").contains("let mut value"));
}

#[cargo_test]
fn fixes_denied_lints_with_compiler_error_codes() {
    let p = project()
        .file(
            "src/lib.rs",
            "#![allow(unused_variables, non_snake_case)]\nenum Choice { Value }\npub fn check() { match Choice::Value { Value => {} } }\n",
        )
        .build();

    p.cargo_("fixit --allow-no-vcs").run();

    assert!(p.read_file("src/lib.rs").contains("Choice::Value =>"));
}

fn package_fingerprints(project: &Project, packages: &[&str]) -> Vec<(String, Vec<u8>)> {
    let mut fingerprints = std::fs::read_dir(project.build_dir().join("debug/.fingerprint"))
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| {
            let name = entry.file_name();
            packages
                .iter()
                .any(|package| name.to_string_lossy().starts_with(package))
        })
        .map(|entry| {
            let dep_info = std::fs::read_dir(entry.path())
                .unwrap()
                .map(Result::unwrap)
                .find(|file| file.file_name().to_string_lossy().starts_with("dep-lib-"))
                .unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(dep_info.path()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    fingerprints.sort_unstable();
    fingerprints
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
