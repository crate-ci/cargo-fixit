use cargo_test_support::cargo_test;
use cargo_test_support::{basic_manifest, compare::assert_ui, project, Project};
use snapbox::str;
use snapbox::IntoData as _;

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
fn single_package_loads_dependency_metadata() {
    let p = project()
        .file(
            "src/lib.rs",
            "pub fn sample() { let mut value = 1; let _ = value; }\n",
        )
        .file(
            "src/main.rs",
            "fn main() { let mut value = 1; let _ = value; }\n",
        )
        .build();

    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.arg("--allow-no-vcs");
    command.env("CARGO", p.root().join("metadata-must-not-run"));
    command.env("FIXIT_LOG", "cargo_fixit=warn");

    cargo_test_support::execs()
        .with_process_builder(command)
        .with_stderr_data(str![[r#"
[..] WARN cargo_fixit::ops::fixit: failed to run `cargo metadata`: [..]
[CHECKING] foo v0.0.1
[FIXED] src/lib.rs (1 fix)
[FIXED] src/main.rs (1 fix)

"#]])
        .run();

    assert!(!p.read_file("src/lib.rs").contains("let mut value"));
    assert!(!p.read_file("src/main.rs").contains("let mut value"));
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
fn restores_prior_writes_when_later_write_fails() {
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
    assert_eq!(p.read_file("src/module.rs"), original_module);
}

#[cfg(unix)]
#[cargo_test]
fn restores_all_files_when_batched_write_fails() {
    use std::os::unix::fs::PermissionsExt;

    let original_a = "pub fn a() -> i32 { let mut value = 1; value }\n";
    let original_b = "pub fn b() -> i32 { let mut value = 1; value }\n";
    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
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
[ERROR] failed to write `b/src/lib.rs`: [..]

"#]])
        .run();

    std::fs::set_permissions(&unwritable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(p.read_file("a/src/lib.rs"), original_a);
    assert_eq!(p.read_file("b/src/lib.rs"), original_b);
}

#[cargo_test]
fn independent_workspace_packages() {
    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
        )
        .file("a/Cargo.toml", &basic_manifest("a", "0.1.0"))
        .file("a/src/lib.rs", "pub fn a() { let mut value = 1; let _ = value; }\n")
        .file("b/Cargo.toml", &basic_manifest("b", "0.1.0"))
        .file("b/src/lib.rs", "pub fn b() { let mut value = 1; let _ = value; }\n")
        .build();

    p.cargo_("fixit --workspace --allow-no-vcs")
        .with_status(0)
        .with_stderr_data(
            str![[r#"
[CHECKING] a v0.1.0
[FIXED] a/src/lib.rs (1 fix)
[CHECKING] b v0.1.0
[FIXED] b/src/lib.rs (1 fix)

"#]]
            .unordered(),
        )
        .run();

    for name in ["a", "b"] {
        assert_eq!(
            p.read_file(format!("{name}/src/lib.rs")),
            format!("pub fn {name}() {{ let value = 1; let _ = value; }}\n")
        );
    }
}

#[cargo_test]
fn hardlinked_workspace_packages() {
    let original = "pub fn sample() -> usize { let mut value = 1; value }\n";
    let fixed = "pub fn sample() -> usize { let value = 1; value }\n";
    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
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

    p.cargo_("fixit --workspace --allow-no-vcs --broken-code")
        .run();

    assert_eq!(p.read_file("a/src/lib.rs"), fixed);
    assert_eq!(p.read_file("b/src/lib.rs"), fixed);
    assert!(same_file::is_same_file(&source, &hardlink).unwrap());
    p.cargo_("check --workspace").run();
}

#[cargo_test]
fn dependency_graph_batches_only_independent_packages() {
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
    let rustc_log = p.root().join("rustc.log");
    std::fs::create_dir_all(&rustc_log).unwrap();

    p.cargo_("build").with_status(0).run();
    p.cargo_("fixit --allow-no-vcs")
        .env("RUSTC_WORKSPACE_WRAPPER", crate::fix::echo_wrapper())
        .env("__CARGO_FIXIT_RUSTC_LOG", &rustc_log)
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

    let crate_invocations = crate::fix::rustc_invocations(&rustc_log, ["a", "b", "c", "d"]);
    // `c` and `d` share a batch, while `b` and then `a` wait for their dependencies.
    assert_eq!(crate_invocations, [4, 3, 2, 2]);

    for package in ["a", "b", "c", "d"] {
        p.change_file(format!("{package}/src/lib.rs"), "use std as foo;");
    }
    std::fs::remove_dir_all(&rustc_log).unwrap();
    std::fs::create_dir_all(&rustc_log).unwrap();

    p.cargo_("fixit --allow-no-vcs --Zdangerous-parallel-fixes")
        .env("RUSTC_WORKSPACE_WRAPPER", crate::fix::echo_wrapper())
        .env("__CARGO_FIXIT_RUSTC_LOG", &rustc_log)
        .with_status(0)
        .run();

    let crate_invocations = crate::fix::rustc_invocations(&rustc_log, ["a", "b", "c", "d"]);
    assert_eq!(crate_invocations, [2, 2, 2, 2]);
}

#[cargo_test]
fn dev_dependency_cycle_is_fixed_sequentially() {
    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = [\"a\", \"b\", \"c\"]\nresolver = \"2\"\n",
        )
        .file(
            "a/Cargo.toml",
            &format!(
                "{}\n[dev-dependencies]\nb = {{ path = '../b' }}\n",
                basic_manifest("a", "0.1.0")
            ),
        )
        .file(
            "a/src/lib.rs",
            "pub fn a() -> usize { let mut value = 1; value }\n",
        )
        .file(
            "a/tests/cycle.rs",
            "#[test]\nfn cycle() { let mut value = b::b(); assert_eq!(value, 1); }\n",
        )
        .file(
            "b/Cargo.toml",
            &format!(
                "{}\n[dependencies]\nc = {{ path = '../c' }}\n",
                basic_manifest("b", "0.1.0")
            ),
        )
        .file(
            "b/src/lib.rs",
            "pub fn b() -> usize { let mut value = c::c(); value }\n",
        )
        .file(
            "c/Cargo.toml",
            &format!(
                "{}\n[dependencies]\na = {{ path = '../a' }}\n",
                basic_manifest("c", "0.1.0")
            ),
        )
        .file(
            "c/src/lib.rs",
            "pub fn c() -> usize { let mut value = a::a(); value }\n",
        )
        .build();

    p.cargo_("fixit --workspace --all-targets --allow-no-vcs")
        .run();

    for path in [
        "a/src/lib.rs",
        "a/tests/cycle.rs",
        "b/src/lib.rs",
        "c/src/lib.rs",
    ] {
        assert!(!p.read_file(path).contains("let mut"));
    }
    p.cargo_("check --workspace --all-targets").run();
}

#[cargo_test]
fn build_unit_order() {
    let p = project()
        .file(
            "Cargo.toml",
            &format!(
                "{}\n[[bin]]\nname = \"app\"\npath = \"src/main.rs\"\n",
                basic_manifest("foo", "0.1.0")
            ),
        )
        .file("build.rs", "fn main(){ let mut a = 1; let _ = a; }")
        .file("src/lib.rs", "fn _a(){ let mut a = 1; let _ = a; }")
        .file("src/main.rs", "fn main(){ let mut a = 1; let _ = a; }")
        .build();
    let rustc_log = p.root().join("rustc.log");
    std::fs::create_dir_all(&rustc_log).unwrap();

    p.cargo_("fixit --allow-no-vcs")
        .env("RUSTC_WORKSPACE_WRAPPER", crate::fix::echo_wrapper())
        .env("__CARGO_FIXIT_RUSTC_LOG", &rustc_log)
        .with_status(0)
        .with_stderr_data(str![[r#"
[CHECKING] foo v0.1.0
[FIXED] build.rs (1 fix)
[FIXED] src/lib.rs (1 fix)
[FIXED] src/main.rs (1 fix)

"#]])
        .run();

    let crate_invocations =
        crate::fix::rustc_invocations(&rustc_log, ["app", "foo", "build_script_build"]);
    // The library is rebuilt with the binary, so both run in all four
    // compiler passes.
    assert_eq!(crate_invocations, [4, 4, 2]);
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
                "use std::mem::replace;\n\ninclude!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\n",
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
            "[workspace]\nmembers = [\"codegen\", \"consumer\"]\nresolver = \"2\"\n",
        )
        .file(
            "codegen/Cargo.toml",
            &format!(
                "{}\n[lib]\nproc-macro = true\n",
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
                "{}\n[dependencies]\ncodegen = {{ path = '../codegen' }}\n",
                basic_manifest("consumer", "0.1.0")
            ),
        )
        .file(
            "consumer/src/lib.rs",
            "extern crate codegen;\n\nuse std::mem::replace;\n\ncodegen::generate!();\n",
        )
        .build();

    p.cargo_("fixit --workspace --allow-no-vcs").run();

    assert!(!p.read_file("codegen/src/lib.rs").contains("let mut marker"));
    assert!(
        p.read_file("consumer/src/lib.rs")
            .contains("use std::mem::replace;")
    );
    p.cargo_("check --workspace").run();
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
