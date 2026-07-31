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
fn fixes_denied_warnings_with_encoded_rustflags() {
    let p = denied_warnings_project();

    p.cargo_("fixit --allow-no-vcs")
        .env("CARGO_ENCODED_RUSTFLAGS", "--cfg\u{1f}fixit_test")
        .run();

    assert!(!p.read_file("src/lib.rs").contains("let mut value"));
}

#[cargo_test]
fn clippy_fixes_denied_warnings_with_encoded_rustflags() {
    let p = denied_warnings_project();

    p.cargo_("fixit --clippy --allow-no-vcs")
        .env("CARGO_ENCODED_RUSTFLAGS", "--cfg\u{1f}fixit_test")
        .run();

    assert!(!p.read_file("src/lib.rs").contains("let mut value"));
}

#[cargo_test]
fn fixes_denied_warnings_with_empty_encoded_rustflags() {
    let p = denied_warnings_project();

    p.cargo_("fixit --allow-no-vcs")
        .env("CARGO_ENCODED_RUSTFLAGS", "")
        .run();

    assert!(!p.read_file("src/lib.rs").contains("let mut value"));
}

#[cargo_test]
fn preserves_explicit_encoded_lint_caps() {
    let p = denied_warnings_project();

    p.cargo_("fixit --allow-no-vcs")
        .env("CARGO_ENCODED_RUSTFLAGS", "--cap-lints=deny")
        .with_status(101)
        .with_stderr_contains("[ERROR] could not compile")
        .run();

    assert!(p.read_file("src/lib.rs").contains("let mut value"));
}

fn denied_warnings_project() -> Project {
    project()
        .file(
            "src/lib.rs",
            "#![deny(warnings)]\npub fn a() { let mut value = 1; let _ = value; }\n",
        )
        .build()
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

    let fingerprints_before = package_fingerprints(&p, &["foo", "cached-dependency"]);
    assert_eq!(fingerprints_before.len(), 2);

    p.cargo_("fixit --allow-no-vcs").run();

    assert_eq!(
        package_fingerprints(&p, &["foo", "cached-dependency"]),
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

    let fingerprints_before = package_fingerprints(&p, &["app", "cached-dependency"]);
    assert_eq!(fingerprints_before.len(), 2);

    p.cargo_("fixit --clippy --workspace --allow-no-vcs").run();

    assert_eq!(
        package_fingerprints(&p, &["app", "cached-dependency"]),
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
    let root = project.build_dir().join("debug");
    let dep_infos = packages
        .iter()
        .map(|package| format!("dep-lib-{}", package.replace('-', "_")))
        .collect::<Vec<_>>();
    let mut directories = vec![root.clone()];
    let mut fingerprints = Vec::new();

    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(directory).unwrap().map(Result::unwrap) {
            if entry.file_type().unwrap().is_dir() {
                directories.push(entry.path());
            } else if dep_infos
                .iter()
                .any(|dep_info| entry.file_name() == std::ffi::OsStr::new(dep_info))
            {
                let path = entry.path();
                fingerprints.push((
                    path.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(path).unwrap(),
                ));
            }
        }
    }
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

#[cfg(unix)]
#[cargo_test]
fn retains_prior_writes_when_later_write_fails() {
    use std::os::unix::fs::PermissionsExt;

    let original_lib = "mod module;\npub fn lib() { let mut value = 1; let _ = value; }\n";
    let original_module = "pub fn module() { let mut value = 1; let _ = value; }\n";
    let p = project()
        .file("Cargo.toml", &basic_manifest("foo", "0.1.0"))
        .file("src/lib.rs", original_lib)
        .file("src/module.rs", original_module)
        .build();

    let unwritable = p.root().join("src/lib.rs");
    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o444)).unwrap();

    p.cargo_("fixit --allow-no-vcs")
        .with_status(101)
        .with_stderr_data(str![[r#"
[ERROR] failed to write `src/lib.rs`: [..]

"#]])
        .run();

    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(p.read_file("src/lib.rs"), original_lib);
    assert_eq!(
        p.read_file("src/module.rs"),
        "pub fn module() { let value = 1; let _ = value; }\n"
    );
}

#[cfg(unix)]
#[cargo_test]
fn retains_completed_target_when_later_write_fails() {
    use std::os::unix::fs::PermissionsExt;

    let original_a = "pub fn a() -> i32 { let mut value = 1; value }\n";
    let original_b = "pub fn b() -> i32 { let mut value = a::a(); value }\n";
    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
        )
        .file("a/Cargo.toml", &basic_manifest("a", "0.1.0"))
        .file("a/src/lib.rs", original_a)
        .file(
            "b/Cargo.toml",
            "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\na = { path = \"../a\" }\n",
        )
        .file("b/src/lib.rs", original_b)
        .build();

    let unwritable = p.root().join("b/src/lib.rs");
    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o444)).unwrap();

    p.cargo_("fixit --workspace --allow-no-vcs")
        .with_status(101)
        .with_stderr_data(str![[r#"
[CHECKING] a v0.1.0
[FIXED] a/src/lib.rs (1 fix)
[ERROR] failed to write `b/src/lib.rs`: [..]

"#]])
        .run();

    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        p.read_file("a/src/lib.rs"),
        "pub fn a() -> i32 { let value = 1; value }\n"
    );
    assert_eq!(p.read_file("b/src/lib.rs"), original_b);
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
