use cargo_test_support::basic_manifest;
use cargo_test_support::cargo_test;
use cargo_test_support::compare::assert_ui;
use cargo_test_support::project;
use cargo_test_support::Project;
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
fn fixes_denied_warnings() {
    let p = denied_warnings_project();

    p.cargo_("fixit --allow-no-vcs").run();

    assert!(!p.read_file("src/lib.rs").contains("let mut value"));
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
            "#![deny(warnings)]
pub fn a() { let mut value = 1; let _ = value; }
",
        )
        .build()
}

#[cargo_test]
fn reuse_checks_cache() {
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

    p.cargo_("fixit --allow-no-vcs --verbose")
        .with_stderr_data(str![])
        .run();
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
[CHECKING] a v0.1.0
[FIXED] a/src/lib.rs (1 fix)
[WARNING] function `bar` is never used
 --> a/src/lib.rs:1:5
  |
1 |  fn bar() {}
  |     ^^^
  |
  = [NOTE] `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

[CHECKING] b v0.1.0
[FIXED] b/src/lib.rs (1 fix)
[WARNING] function `bar` is never used
 --> b/src/lib.rs:1:5
  |
1 |  fn bar() {}
  |     ^^^
  |
  = [NOTE] `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default


"#]])
        .run();
}

#[cargo_test]
fn metadata_error() {
    let p = project()
        .file("Cargo.toml", "[")
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
        .with_status(101)
        .with_stderr_data(str![[r#"
[ERROR] unquoted keys cannot be empty, expected letters, numbers, `-`, `_`
 --> Cargo.toml:1:2
  |
1 | [
  |  ^
[ERROR] could not compile

"#]])
        .run();
}

#[cargo_test]
fn non_json_error() {
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

    p.cargo_("fixit --allow-no-vcs --bin foo")
        .with_status(101)
        .with_stderr_data(str![[r#"
[ERROR] no bin target named `foo` in default-run packages
[ERROR] could not compile

"#]])
        .run();
}

#[cfg(unix)]
#[cargo_test]
fn restores_prior_writes_when_later_write_fails() {
    use std::os::unix::fs::PermissionsExt;

    let original_lib = "mod module;
pub fn lib() { let mut value = 1; let _ = value; }
";
    let original_module = "pub fn module() { let mut value = 1; let _ = value; }
";
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
[ERROR] failed to write `src/lib.rs`: Permission denied (os error 13)

"#]])
        .run();

    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(p.read_file("src/lib.rs"), original_lib);
    assert_eq!(p.read_file("src/module.rs"), original_module);
}

#[cfg(unix)]
#[cargo_test]
fn restores_all_files_when_batched_write_fails() {
    use std::os::unix::fs::PermissionsExt;

    let original_a = "pub fn a() -> i32 { let mut value = 1; value }
";
    let original_b = "pub fn b() -> i32 { let mut value = 1; value }
";
    let p = project()
        .file(
            "Cargo.toml",
            r#"[workspace]
members = ["a", "b"]
resolver = "2"
"#,
        )
        .file("a/Cargo.toml", &basic_manifest("a", "0.1.0"))
        .file("a/src/lib.rs", original_a)
        .file("b/Cargo.toml", &basic_manifest("b", "0.1.0"))
        .file("b/src/lib.rs", original_b)
        .build();

    let unwritable = p.root().join("b/src/lib.rs");
    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o444)).unwrap();

    p.cargo_("fixit --workspace --allow-no-vcs")
        .with_status(101)
        .with_stderr_data(str![[r#"
[ERROR] failed to write `b/src/lib.rs`: Permission denied (os error 13)

"#]])
        .run();

    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(p.read_file("a/src/lib.rs"), original_a);
    assert_eq!(p.read_file("b/src/lib.rs"), original_b);
}

#[cargo_test]
fn fix_order_build_unit() {
    let p = project()
        .file(
            "Cargo.toml",
            &format!(
                "{}
[[bin]]
name = \"app\"
path = \"src/main.rs\"
",
                basic_manifest("foo", "0.1.0")
            ),
        )
        .file("build.rs", "fn main(){ let mut a = 1; let _ = a; }")
        .file("src/lib.rs", "fn _a(){ let mut a = 1; let _ = a; }")
        .file("src/main.rs", "fn main(){ let mut a = 1; let _ = a; }")
        .build();

    p.cargo_("fixit --allow-no-vcs --verbose")
        .with_stderr_data(str![[r#"
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - build-script-build (custom-build)
     Checked foo v0.1.0 - foo (lib)
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - foo (lib)
[CHECKING] foo v0.1.0
[FIXED] src/main.rs (1 fix)
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - build-script-build (custom-build)
     Checked foo v0.1.0 - foo (lib)
[FIXED] build.rs (1 fix)
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - foo (lib)
[FIXED] src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn fix_order_multiple_lib_crate_types() {
    let p = project()
        .file(
            "Cargo.toml",
            &format!(
                r#"{}
[lib]
crate-type = ["rlib", "cdylib"]
"#,
                basic_manifest("foo", "0.1.0")
            ),
        )
        .file(
            "src/lib.rs",
            "pub fn foo() { let mut value = 1; let _ = value; }",
        )
        .build();

    p.cargo_("fixit --allow-no-vcs --verbose")
        .with_stderr_data(str![[r#"
     Checked foo v0.1.0 - foo (lib)
     Checked foo v0.1.0 - foo (lib)
[CHECKING] foo v0.1.0
[FIXED] src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn fix_order_host_target_dependency() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"[workspace]
members = ["app", "dep"]
resolver = "2"
"#,
        )
        .file(
            "app/Cargo.toml",
            &format!(
                "{}
[build-dependencies]
dep = {{ path = '../dep' }}

[dependencies]
dep = {{ path = '../dep' }}
",
                basic_manifest("app", "0.1.0")
            ),
        )
        .file(
            "app/build.rs",
            "fn main() { let mut value = dep::dep(); let _ = value; }\n",
        )
        .file("app/src/main.rs", "fn main(){ let mut a = 1; let _ = a; }")
        .file("dep/Cargo.toml", &basic_manifest("dep", "0.1.0"))
        .file(
            "dep/src/lib.rs",
            "pub fn dep() -> usize { let mut value = 1; value }\n",
        )
        .build();

    p.cargo_("fixit --workspace --allow-no-vcs --target host-tuple --verbose")
        .with_stderr_data(str![[r#"
     Checked app v0.1.0 - app (bin)
     Checked app v0.1.0 - build-script-build (custom-build)
     Checked dep v0.1.0 - dep (lib)
     Checked dep v0.1.0 - dep (lib)
     Checked app v0.1.0 - app (bin)
[CHECKING] app v0.1.0
[FIXED] app/src/main.rs (1 fix)
     Checked app v0.1.0 - app (bin)
     Checked app v0.1.0 - build-script-build (custom-build)
[FIXED] app/build.rs (1 fix)
     Checked app v0.1.0 - app (bin)
     Checked app v0.1.0 - build-script-build (custom-build)
     Checked dep v0.1.0 - dep (lib)
     Checked dep v0.1.0 - dep (lib)
[CHECKING] dep v0.1.0
[FIXED] dep/src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn fix_order_unit_test() {
    let p = project()
        .file(
            "Cargo.toml",
            &format!(
                "{}
[[bin]]
name = \"app\"
path = \"src/main.rs\"
",
                basic_manifest("foo", "0.1.0")
            ),
        )
        .file(
            "src/lib.rs",
            "
fn _a(){ let mut a = 1; let _ = a; }

#[test]
fn foo() {
    let mut a = 1;
    let _ = a;
}
",
        )
        .file(
            "src/main.rs",
            "
fn main(){ let mut a = 1; let _ = a; }

#[test]
fn foo() {
    let mut a = 1;
    let _ = a;
}
",
        )
        .file(
            "tests/test_a.rs",
            "
fn _a(){ let mut a = 1; let _ = a; }

#[test]
fn foo() {
    let mut a = 1;
    let _ = a;
}
",
        )
        .file(
            "tests/test_b.rs",
            "
fn _a(){ let mut a = 1; let _ = a; }

#[test]
fn foo() {
    let mut a = 1;
    let _ = a;
}
",
        )
        .file(
            "examples/examp_a.rs",
            "
fn main(){ let mut a = 1; let _ = a; }
",
        )
        .file(
            "examples/examp_b.rs",
            "
fn main(){ let mut a = 1; let _ = a; }
",
        )
        .build();

    p.cargo_("fixit --allow-no-vcs --all-targets --verbose")
        .with_stderr_data(str![[r#"
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - test_a (test)
     Checked foo v0.1.0 - test_b (test)
     Checked foo v0.1.0 - examp_a (example)
     Checked foo v0.1.0 - examp_b (example)
     Checked foo v0.1.0 - foo (lib)
     Checked foo v0.1.0 - foo (lib)
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - app (bin)
[CHECKING] foo v0.1.0
[FIXED] src/main.rs (2 fixes)
     Checked foo v0.1.0 - test_a (test)
[FIXED] tests/test_a.rs (2 fixes)
     Checked foo v0.1.0 - test_b (test)
[FIXED] tests/test_b.rs (2 fixes)
     Checked foo v0.1.0 - examp_a (example)
[FIXED] examples/examp_a.rs (1 fix)
     Checked foo v0.1.0 - examp_b (example)
[FIXED] examples/examp_b.rs (1 fix)
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - app (bin)
     Checked foo v0.1.0 - test_a (test)
     Checked foo v0.1.0 - test_b (test)
     Checked foo v0.1.0 - examp_a (example)
     Checked foo v0.1.0 - examp_b (example)
     Checked foo v0.1.0 - foo (lib)
     Checked foo v0.1.0 - foo (lib)
[FIXED] src/lib.rs (2 fixes)

"#]])
        .run();
}

#[cargo_test]
fn fix_order_independent_packages() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"[workspace]
members = ["a", "b"]
resolver = "2"
"#,
        )
        .file("a/Cargo.toml", &basic_manifest("a", "0.1.0"))
        .file(
            "a/src/lib.rs",
            "pub fn a() { let mut value = 1; let _ = value; }
",
        )
        .file("b/Cargo.toml", &basic_manifest("b", "0.1.0"))
        .file(
            "b/src/lib.rs",
            "pub fn b() { let mut value = 1; let _ = value; }
",
        )
        .build();

    p.cargo_("fixit --workspace --allow-no-vcs --verbose")
        .with_status(0)
        .with_stderr_data(str![[r#"
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
[CHECKING] a v0.1.0
[FIXED] a/src/lib.rs (1 fix)
[CHECKING] b v0.1.0
[FIXED] b/src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn fix_order_serial_packages() {
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
                c = { path = "../c" }
            "#,
        )
        .file("b/src/lib.rs", "use std as foo;")
        .file(
            "c/Cargo.toml",
            r#"
                [package]
                name = "c"
                version = "0.1.0"
                edition = "2024"

                [dependencies]
                d = { path = "../d" }
            "#,
        )
        .file("c/src/lib.rs", "use std as foo;")
        .file("d/Cargo.toml", &basic_manifest("d", "0.1.0"))
        .file("d/src/lib.rs", "use std as foo;")
        .build();

    p.cargo_("fixit --workspace --allow-no-vcs --verbose")
        .with_status(0)
        .with_stderr_data(str![[r#"
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked c v0.1.0 - c (lib)
     Checked d v0.1.0 - d (lib)
     Checked a v0.1.0 - a (lib)
[CHECKING] a v0.1.0
[FIXED] a/src/lib.rs (1 fix)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
[CHECKING] b v0.1.0
[FIXED] b/src/lib.rs (1 fix)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked c v0.1.0 - c (lib)
[CHECKING] c v0.1.0
[FIXED] c/src/lib.rs (1 fix)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked c v0.1.0 - c (lib)
     Checked d v0.1.0 - d (lib)
[CHECKING] d v0.1.0
[FIXED] d/src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn fix_order_hardlinked_workspace_packages() {
    let original = "pub fn sample() -> usize { let mut value = 1; value }
";
    let p = project()
        .file(
            "Cargo.toml",
            r#"[workspace]
members = ["a", "b"]
resolver = "2"
"#,
        )
        .file("a/Cargo.toml", &basic_manifest("a", "0.1.0"))
        .file("a/src/lib.rs", original)
        .file("b/Cargo.toml", &basic_manifest("b", "0.1.0"))
        .file("b/src/lib.rs", "")
        .build();

    let source = p.root().join("a/src/lib.rs");
    let hardlink = p.root().join("b/src/lib.rs");
    std::fs::remove_file(&hardlink).unwrap();
    std::fs::hard_link(&source, &hardlink).unwrap();

    p.cargo_("fixit --workspace --allow-no-vcs --broken-code --verbose")
        .with_stderr_data(str![[r#"
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
[CHECKING] a v0.1.0
[FIXED] a/src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn fix_order_dev_dependency_cycle_is_fixed_sequentially() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"[workspace]
members = ["a", "b", "c"]
resolver = "2"
"#,
        )
        .file(
            "a/Cargo.toml",
            &format!(
                "{}
[dev-dependencies]
b = {{ path = '../b' }}
",
                basic_manifest("a", "0.1.0")
            ),
        )
        .file(
            "a/src/lib.rs",
            "pub fn a() -> usize { let mut value = 1; value }
",
        )
        .file(
            "a/tests/cycle.rs",
            "#[test]
fn cycle() { let mut value = b::b(); assert_eq!(value, 1); }
",
        )
        .file(
            "b/Cargo.toml",
            &format!(
                "{}
[dependencies]
c = {{ path = '../c' }}
",
                basic_manifest("b", "0.1.0")
            ),
        )
        .file(
            "b/src/lib.rs",
            "pub fn b() -> usize { let mut value = c::c(); value }
",
        )
        .file(
            "c/Cargo.toml",
            &format!(
                "{}
[dependencies]
a = {{ path = '../a' }}
",
                basic_manifest("c", "0.1.0")
            ),
        )
        .file(
            "c/src/lib.rs",
            "pub fn c() -> usize { let mut value = a::a(); value }
",
        )
        .build();

    p.cargo_("fixit --workspace --all-targets --allow-no-vcs --verbose")
        .with_stderr_data(str![[r#"
     Checked a v0.1.0 - cycle (test)
     Checked a v0.1.0 - a (lib)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked b v0.1.0 - b (lib)
     Checked c v0.1.0 - c (lib)
     Checked c v0.1.0 - c (lib)
     Checked a v0.1.0 - cycle (test)
[CHECKING] a v0.1.0
[FIXED] a/tests/cycle.rs (1 fix)
     Checked a v0.1.0 - cycle (test)
     Checked a v0.1.0 - a (lib)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked b v0.1.0 - b (lib)
     Checked c v0.1.0 - c (lib)
     Checked c v0.1.0 - c (lib)
[FIXED] a/src/lib.rs (1 fix)
     Checked a v0.1.0 - cycle (test)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked b v0.1.0 - b (lib)
[CHECKING] b v0.1.0
[FIXED] b/src/lib.rs (1 fix)
     Checked a v0.1.0 - cycle (test)
     Checked a v0.1.0 - a (lib)
     Checked b v0.1.0 - b (lib)
     Checked b v0.1.0 - b (lib)
     Checked c v0.1.0 - c (lib)
     Checked c v0.1.0 - c (lib)
[CHECKING] c v0.1.0
[FIXED] c/src/lib.rs (1 fix)

"#]])
        .run();
}

#[cargo_test]
fn crate_fixed_on_two_targets() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"[workspace]
members = ["app", "shared"]
resolver = "2"
"#,
        )
        .file(
            "app/Cargo.toml",
            &format!(
                "{}
[dependencies]
shared = {{ path = '../shared' }}

[build-dependencies]
shared = {{ path = '../shared' }}
",
                basic_manifest("app", "0.1.0")
            ),
        )
        .file("app/build.rs", "fn main() { shared::shared(); }\n")
        .file("app/src/lib.rs", "pub fn app() { shared::shared(); }\n")
        .file("shared/Cargo.toml", &basic_manifest("shared", "0.1.0"))
        .file(
            "shared/src/lib.rs",
            "pub fn shared() { let mut value = 1; let _ = value; }\n",
        )
        .build();

    p.cargo_("fixit --workspace --target host-tuple --allow-no-vcs --verbose")
        .with_stderr_data(str![[r#"
     Checked app v0.1.0 - build-script-build (custom-build)
     Checked app v0.1.0 - app (lib)
     Checked shared v0.1.0 - shared (lib)
     Checked shared v0.1.0 - shared (lib)
[CHECKING] app v0.1.0
     Checked app v0.1.0 - build-script-build (custom-build)
     Checked app v0.1.0 - app (lib)
     Checked shared v0.1.0 - shared (lib)
     Checked shared v0.1.0 - shared (lib)
[CHECKING] shared v0.1.0
[FIXED] shared/src/lib.rs (1 fix)

"#]])
        .run();

    assert!(!p.read_file("shared/src/lib.rs").contains("let mut value"));
    p.cargo_("check --workspace --target host-tuple").run();
}

#[cargo_test]
fn build_script_fixes_refresh_generated_source_before_downstream_fixes() {
    let build_script = r#"
        use std::env;
        use std::fs;

        fn main() {
            let mut marker = 1usize;
            let _ = marker;

            let generated = if include_str!("build.rs").contains(concat!("let mut ", "marker")) {
                "pub fn generated() {}"
            } else {
                "pub fn generated() { let mut value = 0; let _ = replace(&mut value, 1); }"
            };
            fs::write(env::var("OUT_DIR").unwrap() + "/generated.rs", generated).unwrap();
            println!("cargo:rerun-if-changed=build.rs");
        }
    "#;

    for (name, args) in [
        ("normal", "fixit --allow-no-vcs"),
        ("broken", "fixit --allow-no-vcs --broken-code"),
    ] {
        let p = project()
            .at(format!("build-script-{name}"))
            .file("Cargo.toml", &basic_manifest("foo", "0.1.0"))
            .file("build.rs", build_script)
            .file(
                "src/lib.rs",
                r#"use std::mem::replace;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));
"#,
            )
            .build();

        p.cargo_(args).run();

        assert!(!p.read_file("build.rs").contains("let mut marker"));
        assert!(p.read_file("src/lib.rs").contains("use std::mem::replace;"));
        p.cargo_("check").run();
    }
}

#[cargo_test]
fn proc_macro_fixes_refresh_expansions_before_downstream_fixes() {
    let p = project()
        .file(
            "Cargo.toml",
            r#"[workspace]
members = ["codegen", "consumer"]
resolver = "2"
"#,
        )
        .file(
            "codegen/Cargo.toml",
            &format!(
                "{}
[lib]
proc-macro = true
",
                basic_manifest("codegen", "0.1.0")
            ),
        )
        .file(
            "codegen/src/lib.rs",
            r#"
                extern crate proc_macro;

                use proc_macro::TokenStream;

                #[proc_macro]
                pub fn generate(_: TokenStream) -> TokenStream {
                    let mut marker = 0;
                    let _ = marker;

                    if include_str!("lib.rs").contains(concat!("let mut ", "marker")) {
                        TokenStream::new()
                    } else {
                        "pub fn generated() { let mut value = 0; let _ = replace(&mut value, 1); }"
                            .parse()
                            .unwrap()
                    }
                }
            "#,
        )
        .file(
            "consumer/Cargo.toml",
            &format!(
                "{}
[dependencies]
codegen = {{ path = '../codegen' }}
",
                basic_manifest("consumer", "0.1.0")
            ),
        )
        .file(
            "consumer/src/lib.rs",
            "extern crate codegen;

use std::mem::replace;

codegen::generate!();
",
        )
        .build();

    p.cargo_("fixit --workspace --allow-no-vcs").run();

    assert!(!p.read_file("codegen/src/lib.rs").contains("let mut marker"));
    assert!(p
        .read_file("consumer/src/lib.rs")
        .contains("use std::mem::replace;"));
    p.cargo_("check --workspace").run();
}
