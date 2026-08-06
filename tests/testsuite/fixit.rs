use cargo_test_support::Project;
use cargo_test_support::basic_manifest;
use cargo_test_support::cargo_test;
use cargo_test_support::compare::assert_ui;
use cargo_test_support::project;
use snapbox::IntoData as _;
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
fn cargo_env() {
    let fake_cargo = fake_cargo();

    let p = project()
        .file("src/lib.rs", "pub fn sample() {}")
        .file("fake-cargo-called", "not called")
        .build();
    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.arg("--allow-no-vcs");
    command.env("CARGO", fake_cargo.bin("fake-cargo"));
    command.env(
        "CARGO_FIXIT_FAKE_CARGO_MARKER",
        p.root().join("fake-cargo-called"),
    );
    cargo_test_support::execs()
        .with_process_builder(command)
        .run();

    assert_ui().eq(
        p.read_file("fake-cargo-called"),
        str![[r#"
check
--message-format
json-diagnostic-rendered-ansi
"#]],
    );
}

#[cargo_test]
fn single_package_loads_primary_package_metadata() {
    let fake_cargo = fake_cargo();

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
    let metadata_log = p.root().join("metadata.log");

    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.arg("--allow-no-vcs");
    command.env("CARGO", fake_cargo.bin("fake-cargo"));
    command.env("FIXIT_REAL_CARGO", env!("CARGO"));
    command.env("FIXIT_METADATA_LOG", &metadata_log);
    command.env("FIXIT_LOG", "cargo_fixit=warn");

    cargo_test_support::execs()
        .with_process_builder(command)
        .with_stderr_data(str![[r#"
[CHECKING] foo v0.0.1
[FIXED] src/lib.rs (1 fix)
[FIXED] src/main.rs (1 fix)

"#]])
        .run();

    assert!(!p.read_file("src/lib.rs").contains("let mut value"));
    assert!(!p.read_file("src/main.rs").contains("let mut value"));
    assert_ui().eq(
        p.read_file("metadata.log"),
        str![[r#"
metadata
--format-version
1
--no-deps
"#]],
    );
}

fn fake_cargo() -> Project {
    let fake_cargo = project()
        .at("fake-cargo")
        .file("Cargo.toml", &basic_manifest("fake-cargo", "1.0.0"))
        .file(
            "src/main.rs",
            r#"
            fn main() {
                let args = std::env::args_os().skip(1).collect::<Vec<_>>();
                if args.first().is_some_and(|arg| arg == "check") {
                    if let Some(log) = std::env::var_os("FIXIT_CHECK_LOG") {
                        use std::io::Write as _;

                        let mut log = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(log)
                            .unwrap();
                        writeln!(log, "check").unwrap();
                    }
                }
                if args.first().is_some_and(|arg| arg == "metadata") {
                    if let Some(log) = std::env::var_os("FIXIT_METADATA_LOG") {
                        let args = args
                            .iter()
                            .map(|arg| arg.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("\n");
                        std::fs::write(log, args).unwrap();
                    }
                }

                if let Some(marker) = std::env::var_os("CARGO_FIXIT_FAKE_CARGO_MARKER") {
                    let args = args
                        .iter()
                        .map(|arg| arg.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join("\n");
                    std::fs::write(marker, args).unwrap();
                } else {
                    let cargo = std::env::var_os("FIXIT_REAL_CARGO").unwrap();
                    let status = std::process::Command::new(cargo)
                        .args(args)
                        .env_remove("CARGO")
                        .status()
                        .unwrap();
                    std::process::exit(status.code().unwrap_or(1));
                }
            }
            "#,
        )
        .build();
    fake_cargo.cargo_("build").run();
    fake_cargo
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
        .file(
            "a/src/lib.rs",
            "pub fn a() { let mut value = 1; let _ = value; }\n",
        )
        .file("b/Cargo.toml", &basic_manifest("b", "0.1.0"))
        .file(
            "b/src/lib.rs",
            "pub fn b() { let mut value = 1; let _ = value; }\n",
        )
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
fn independent_packages_progress_while_another_retries() {
    let fake_cargo = fake_cargo();
    let p = rolling_wavefront_project();
    let check_log = p.root().join("check.log");
    let rustc_log = p.root().join("rustc.log");
    std::fs::create_dir_all(&rustc_log).unwrap();

    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.arg("--workspace");
    command.arg("--allow-no-vcs");
    command.env("CARGO", fake_cargo.bin("fake-cargo"));
    command.env("FIXIT_REAL_CARGO", env!("CARGO"));
    command.env("FIXIT_CHECK_LOG", &check_log);
    command.env("RUSTC_WORKSPACE_WRAPPER", crate::fix::echo_wrapper());
    command.env("__CARGO_FIXIT_RUSTC_LOG", &rustc_log);
    cargo_test_support::execs()
        .with_process_builder(command)
        .run();

    for path in [
        "dependency/src/lib.rs",
        "consumer/src/lib.rs",
        "slow/src/lib.rs",
    ] {
        assert!(!p.read_file(path).contains("let mut"));
    }
    assert_eq!(p.read_file("check.log").lines().count(), 3);
    assert_eq!(
        crate::fix::rustc_invocations(&rustc_log, ["dependency", "consumer", "slow"]),
        [2, 3, 3]
    );
}

#[cargo_test]
fn rolling_wavefront_restores_retired_packages_on_failure() {
    let p = rolling_wavefront_project();
    let original_dependency = p.read_file("dependency/src/lib.rs");
    let original_consumer = p.read_file("consumer/src/lib.rs");
    let original_slow = p.read_file("slow/src/lib.rs");

    p.cargo_("fixit --workspace --allow-no-vcs")
        .env("FIXIT_BREAK_AFTER_FIXES", "1")
        .with_status(101)
        .with_stderr_contains("[NOTE] reverting `dependency/src/lib.rs` to its original state")
        .run();

    assert_eq!(p.read_file("dependency/src/lib.rs"), original_dependency);
    assert_eq!(p.read_file("consumer/src/lib.rs"), original_consumer);
    assert_eq!(p.read_file("slow/src/lib.rs"), original_slow);
}

#[cargo_test]
fn rolling_wavefront_restores_shared_files_in_reverse_order() {
    let p = rolling_wavefront_project();
    p.change_file(
        "dependency/Cargo.toml",
        &format!(
            "{}\n[features]\ndefault = ['first']\nfirst = []\nsecond = []\n",
            basic_manifest("dependency", "0.1.0")
        ),
    );
    p.change_file(
        "dependency/src/lib.rs",
        "#[cfg(feature = \"first\")]\npub fn dependency() -> usize { let mut first = 1; first }\n\n#[cfg(feature = \"second\")]\npub fn shared() -> usize { let mut second = 1; second }\n",
    );
    p.change_file(
        "consumer/Cargo.toml",
        &format!(
            "{}\n[features]\ndefault = ['second']\nfirst = []\nsecond = []\n\n[dependencies]\ndependency = {{ path = '../dependency' }}\n",
            basic_manifest("consumer", "0.1.0")
        ),
    );
    p.change_file(
        "consumer/src/lib.rs",
        "#[path = \"../../dependency/src/lib.rs\"]\nmod shared;\npub fn consumer() -> usize { dependency::dependency() + shared::shared() }\n",
    );
    let original_shared = p.read_file("dependency/src/lib.rs");

    p.cargo_("fixit --workspace --allow-no-vcs")
        .env("FIXIT_BREAK_AFTER_FIXES", "1")
        .with_status(101)
        .with_stderr_contains("[NOTE] reverting `dependency/src/lib.rs` to its original state")
        .run();

    assert_eq!(p.read_file("dependency/src/lib.rs"), original_shared);
}

fn rolling_wavefront_project() -> Project {
    project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = ['dependency', 'consumer', 'slow']\nresolver = '2'\n",
        )
        .file("dependency/Cargo.toml", &basic_manifest("dependency", "0.1.0"))
        .file(
            "dependency/src/lib.rs",
            "pub fn dependency() -> usize { let mut value = 1; value }\n",
        )
        .file(
            "consumer/Cargo.toml",
            &format!(
                "{}\n[dependencies]\ndependency = {{ path = '../dependency' }}\n",
                basic_manifest("consumer", "0.1.0")
            ),
        )
        .file(
            "consumer/src/lib.rs",
            "pub fn consumer() -> usize { let mut value = dependency::dependency(); value }\n",
        )
        .file("slow/Cargo.toml", &basic_manifest("slow", "0.1.0"))
        .file(
            "slow/build.rs",
            r#"
                fn main() {
                    println!("cargo:rustc-check-cfg=cfg(fixit_first)");
                    println!("cargo:rustc-check-cfg=cfg(fixit_second)");
                    println!("cargo:rustc-check-cfg=cfg(fixit_broken)");
                    println!("cargo:rerun-if-changed=src/lib.rs");

                    let source = std::fs::read_to_string("src/lib.rs").unwrap();
                    if source.contains("let mut first") {
                        println!("cargo:rustc-cfg=fixit_first");
                    } else if source.contains("let mut second") {
                        println!("cargo:rustc-cfg=fixit_second");
                    } else if std::env::var_os("FIXIT_BREAK_AFTER_FIXES").is_some() {
                        println!("cargo:rustc-cfg=fixit_broken");
                    }
                }
            "#,
        )
        .file(
            "slow/src/lib.rs",
            r#"
                #[cfg(fixit_first)]
                pub fn slow() -> usize { let mut first = 1; first }

                #[cfg(fixit_second)]
                pub fn slow() -> usize { let mut second = 1; second }

                #[cfg(not(any(fixit_first, fixit_second)))]
                pub fn slow() -> usize { 1 }

                #[cfg(fixit_broken)]
                pub fn broken() -> usize { missing_value() }
            "#,
        )
        .build()
}

#[cargo_test]
fn executable_leaf_targets_batch_only_when_independence_is_assumed() {
    let fake_cargo = fake_cargo();

    for (name, assume_independent, broken_code, expected_checks) in [
        ("default", false, false, 3),
        ("parallel", true, false, 2),
        ("broken", true, true, 3),
    ] {
        let p = project()
            .at(format!("leaf-targets-{name}"))
            .file(
                "src/bin/first.rs",
                "fn main() { let mut value = 1; let _ = value; }\n",
            )
            .file(
                "src/bin/second.rs",
                "fn main() { let mut value = 2; let _ = value; }\n",
            )
            .build();
        let check_log = p.root().join("check.log");

        let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
        command.cwd(p.root());
        command.arg("fixit");
        command.arg("--bins");
        command.arg("--allow-no-vcs");
        if assume_independent {
            command.arg("--Zassume-independent-targets");
        }
        if broken_code {
            command.arg("--broken-code");
        }
        command.env("CARGO", fake_cargo.bin("fake-cargo"));
        command.env("FIXIT_REAL_CARGO", env!("CARGO"));
        command.env("FIXIT_CHECK_LOG", &check_log);
        cargo_test_support::execs()
            .with_process_builder(command)
            .run();

        assert_eq!(p.read_file("check.log").lines().count(), expected_checks);
        for target in ["first", "second"] {
            assert!(!p.read_file(format!("src/bin/{target}.rs")).contains("let mut"));
        }
    }
}

#[cargo_test]
fn executable_leaf_targets_with_shared_sources_remain_serial() {
    let fake_cargo = fake_cargo();
    let p = project()
        .file(
            "src/shared.rs",
            "pub fn shared() -> usize { let mut value = 1; value }\n",
        )
        .file(
            "src/bin/first.rs",
            "#[path = \"../shared.rs\"] mod shared;\nfn main() { let mut value = shared::shared(); let _ = value; }\n",
        )
        .file(
            "src/bin/second.rs",
            "#[path = \"../shared.rs\"] mod shared;\nfn main() { let mut value = shared::shared(); let _ = value; }\n",
        )
        .build();
    let check_log = p.root().join("check.log");

    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.arg("--bins");
    command.arg("--allow-no-vcs");
    command.arg("--Zassume-independent-targets");
    command.env("CARGO", fake_cargo.bin("fake-cargo"));
    command.env("FIXIT_REAL_CARGO", env!("CARGO"));
    command.env("FIXIT_CHECK_LOG", &check_log);
    cargo_test_support::execs()
        .with_process_builder(command)
        .run();

    assert_eq!(p.read_file("check.log").lines().count(), 3);
    for path in ["src/shared.rs", "src/bin/first.rs", "src/bin/second.rs"] {
        assert!(!p.read_file(path).contains("let mut"));
    }
}

#[cargo_test]
fn executable_leaf_targets_retry_serially_after_hidden_source_dependency() {
    let p = project()
        .file("Cargo.toml", &basic_manifest("foo", "0.1.0"))
        .file("build.rs", interdependent_bin_build_script())
        .file(
            "src/bin/first.rs",
            "use std::mem::replace;\n\ninclude!(concat!(env!(\"OUT_DIR\"), \"/first.rs\"));\n\nfn main() { generated(); }\n",
        )
        .file(
            "src/bin/second.rs",
            "use std::mem::replace;\n\ninclude!(concat!(env!(\"OUT_DIR\"), \"/second.rs\"));\n\nfn main() { generated(); }\n",
        )
        .build();

    p.cargo_("fixit --bins --jobs 1 --allow-no-vcs --Zassume-independent-targets")
        .with_stderr_data(str![[r#"
[CHECKING] foo v0.1.0
[FIXED] src/bin/[..].rs (1 fix)

"#]])
        .run();

    assert_ne!(
        p.read_file("src/bin/first.rs")
            .contains("use std::mem::replace;"),
        p.read_file("src/bin/second.rs")
            .contains("use std::mem::replace;"),
    );
    p.cargo_("check --bins").run();
}

#[cargo_test]
fn speculative_leaf_failure_preserves_retired_wavefront_targets() {
    let p = rolling_wavefront_project();
    p.change_file(
        "consumer/src/lib.rs",
        "pub fn consumer() -> usize { dependency::dependency() }\n",
    );
    p.change_file("consumer/build.rs", interdependent_bin_build_script());
    for target in ["first", "second"] {
        p.change_file(
            format!("consumer/src/bin/{target}.rs"),
            &format!(
                "use std::mem::replace;\ninclude!(concat!(env!(\"OUT_DIR\"), \"/{target}.rs\"));\nfn main() {{ generated(); }}\n"
            ),
        );
    }

    p.cargo_(
        "fixit --workspace --lib --bins --jobs 1 --allow-no-vcs --Zassume-independent-targets",
    )
    .with_stderr_data(str![[r#"
[CHECKING] consumer v0.1.0
[CHECKING] dependency v0.1.0
[FIXED] dependency/src/lib.rs (1 fix)
[FIXED] consumer/src/bin/[..].rs (1 fix)
[CHECKING] slow v0.1.0
[FIXED] slow/src/lib.rs (2 fixes)

"#]])
    .run();

    assert!(!p.read_file("dependency/src/lib.rs").contains("let mut"));
    assert!(!p.read_file("slow/src/lib.rs").contains("let mut"));
    assert_ne!(
        p.read_file("consumer/src/bin/first.rs")
            .contains("use std::mem::replace;"),
        p.read_file("consumer/src/bin/second.rs")
            .contains("use std::mem::replace;"),
    );
    p.cargo_("check --workspace --lib --bins").run();
}

fn interdependent_bin_build_script() -> &'static str {
    r#"
        use std::env;
        use std::fs;
        use std::path::Path;

        fn main() {
            let first = fs::read_to_string("src/bin/first.rs").unwrap();
            let second = fs::read_to_string("src/bin/second.rs").unwrap();
            let imported = "use std::mem::replace;";
            let empty = "pub fn generated() {}";
            let used =
                "pub fn generated() { let mut value = 0; let _ = replace(&mut value, 1); }";
            let output = env::var_os("OUT_DIR").unwrap();
            let output = Path::new(&output);

            fs::write(
                output.join("first.rs"),
                if second.contains(imported) { empty } else { used },
            )
            .unwrap();
            fs::write(
                output.join("second.rs"),
                if first.contains(imported) { empty } else { used },
            )
            .unwrap();
            println!("cargo:rerun-if-changed=src/bin/first.rs");
            println!("cargo:rerun-if-changed=src/bin/second.rs");
        }
    "#
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

#[cfg(unix)]
#[cargo_test]
fn workspace_dependency_graph_uses_unresolved_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = ['a', 'b']\nresolver = '2'\n",
        )
        .file("a/Cargo.toml", &basic_manifest("a", "0.1.0"))
        .file(
            "a/src/lib.rs",
            "pub fn a() { let mut value = 1; let _ = value; }\n",
        )
        .file("b/Cargo.toml", &basic_manifest("b", "0.1.0"))
        .file(
            "b/src/lib.rs",
            "pub fn b() { let mut value = 1; let _ = value; }\n",
        )
        .file(
            "metadata-wrapper.sh",
            r#"#!/bin/sh
if [ "$1" = metadata ]; then
    printf '%s\n' "$*" >> "$FIXIT_METADATA_LOG"
    if [ "$4" != --no-deps ]; then
        exit 41
    fi
fi
exec "$FIXIT_REAL_CARGO" "$@"
"#,
        )
        .build();

    let wrapper = p.root().join("metadata-wrapper.sh");
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let metadata_log = p.root().join("metadata.log");
    let rustc_log = p.root().join("rustc.log");
    std::fs::create_dir_all(&rustc_log).unwrap();

    p.cargo_("build --workspace").run();
    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.arg("--workspace");
    command.arg("--allow-no-vcs");
    command.env("CARGO", &wrapper);
    command.env("FIXIT_REAL_CARGO", env!("CARGO"));
    command.env("FIXIT_METADATA_LOG", &metadata_log);
    command.env("RUSTC_WORKSPACE_WRAPPER", crate::fix::echo_wrapper());
    command.env("__CARGO_FIXIT_RUSTC_LOG", &rustc_log);
    cargo_test_support::execs()
        .with_process_builder(command)
        .run();

    assert_ui().eq(
        p.read_file("metadata.log").trim_end(),
        str![[r#"
metadata --format-version 1 --no-deps
"#]],
    );
    assert_eq!(
        crate::fix::rustc_invocations(&rustc_log, ["a", "b"]),
        [2, 2]
    );
}

#[cfg(unix)]
#[cargo_test(nightly, reason = "-Zscript is unstable")]
fn cargo_script_selects_only_script_package() {
    use std::os::unix::fs::PermissionsExt;

    let p = project()
        .file("Cargo.toml", &basic_manifest("dep", "0.1.0"))
        .file(
            "src/lib.rs",
            "pub fn value() -> usize { let mut value = 1; value }\n",
        )
        .file(
            "script.rs",
            r#"---cargo
[package]
edition = "2024"

[dependencies]
dep = { path = "." }
---

fn main() {
    let mut value = dep::value();
    let _ = value;
}
"#,
        )
        .file(
            "metadata-wrapper.sh",
            r#"#!/bin/sh
if [ "$1" = metadata ]; then
    printf '%s\n' "$*" >> "$FIXIT_METADATA_LOG"
fi
exec "$FIXIT_REAL_CARGO" "$@"
"#,
        )
        .build();

    let wrapper = p.root().join("metadata-wrapper.sh");
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let metadata_log = p.root().join("metadata.log");

    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.args(&["-Z", "script"]);
    command.args(&["--manifest-path", "script.rs"]);
    command.arg("--allow-no-vcs");
    command.env(
        "__CARGO_TEST_CHANNEL_OVERRIDE_DO_NOT_USE_THIS",
        "nightly",
    );
    command.env("CARGO", &wrapper);
    command.env("CARGO_ENCODED_RUSTFLAGS", "--cap-lints=warn");
    command.env("FIXIT_REAL_CARGO", env!("CARGO"));
    command.env("FIXIT_METADATA_LOG", &metadata_log);
    command.env("RUSTC_BOOTSTRAP", "1");
    cargo_test_support::execs()
        .with_process_builder(command)
        .run();

    assert_ui().eq(
        p.read_file("metadata.log").trim_end(),
        str![[r#"
metadata --format-version 1 --no-deps -Z script --manifest-path script.rs
"#]],
    );
    assert!(p.read_file("src/lib.rs").contains("let mut value"));
    assert!(!p.read_file("script.rs").contains("let mut value"));
}

#[cfg(unix)]
#[cargo_test]
fn external_path_dependencies_fall_back_to_resolved_metadata() {
    use std::os::unix::fs::PermissionsExt;

    let p = project()
        .file(
            "Cargo.toml",
            "[workspace]\nmembers = ['consumer', 'provider']\nexclude = ['external']\nresolver = '2'\n",
        )
        .file(
            "consumer/Cargo.toml",
            &format!(
                "{}\n[dependencies]\nexternal = {{ path = '../external' }}\n",
                basic_manifest("consumer", "0.1.0")
            ),
        )
        .file(
            "consumer/src/lib.rs",
            "pub fn consumer() -> usize { let mut value = external::value(); value }\n",
        )
        .file(
            "external/Cargo.toml",
            &format!(
                "{}\n[dependencies]\nprovider = {{ path = '../provider' }}\n",
                basic_manifest("external", "0.1.0")
            ),
        )
        .file("external/src/lib.rs", "pub fn value() -> usize { provider::value() }\n")
        .file("provider/Cargo.toml", &basic_manifest("provider", "0.1.0"))
        .file(
            "provider/src/lib.rs",
            "pub fn value() -> usize { let mut value = 1; value }\n",
        )
        .file(
            "metadata-wrapper.sh",
            r#"#!/bin/sh
if [ "$1" = metadata ]; then
    printf '%s\n' "$*" >> "$FIXIT_METADATA_LOG"
fi
exec "$FIXIT_REAL_CARGO" "$@"
"#,
        )
        .build();

    let wrapper = p.root().join("metadata-wrapper.sh");
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let metadata_log = p.root().join("metadata.log");

    let mut command = cargo_test_support::process(env!("CARGO_BIN_EXE_cargo-fixit"));
    command.cwd(p.root());
    command.arg("fixit");
    command.arg("--workspace");
    command.arg("--allow-no-vcs");
    command.env("CARGO", &wrapper);
    command.env("FIXIT_REAL_CARGO", env!("CARGO"));
    command.env("FIXIT_METADATA_LOG", &metadata_log);
    cargo_test_support::execs()
        .with_process_builder(command)
        .run();

    assert_ui().eq(
        p.read_file("metadata.log").trim_end(),
        str![[r#"
metadata --format-version 1 --no-deps
metadata --format-version 1
"#]],
    );
    assert!(!p.read_file("consumer/src/lib.rs").contains("let mut value"));
    assert!(!p.read_file("provider/src/lib.rs").contains("let mut value"));
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

#[cargo_test]
fn non_json_error() {
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
