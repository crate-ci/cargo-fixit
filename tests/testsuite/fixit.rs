use cargo_test_macro::cargo_test;
use cargo_test_support::{compare::assert_ui, project};
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

    p.cargo_("fixit")
        .with_status(0)
        .with_stderr_data(str![[r#"
[WARNING] variable does not need to be mutable
 --> src/lib.rs:3:21
  |
3 |                 let mut b = 10;
  |                     ----^
  |                     |
  |                     [HELP] remove this `mut`
  |
  = [NOTE] `#[warn(unused_mut)]` on by default


"#]])
        .run();
    assert_ui().eq(
        p.read_file("src/lib.rs"),
        str![[r#"

            pub fn a() {
                let mut b = 10;
                let _ = b;
            }
            
"#]],
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

                let c = 10;
            }
            "#,
        )
        .build();

    p.cargo_("fixit")
        .with_status(0)
        .with_stderr_data(str![[r#"
[WARNING] unused variable: `c`
 --> src/lib.rs:6:21
  |
6 |                 let c = 10;
  |                     ^ [HELP] if this is intentional, prefix it with an underscore: `_c`
  |
  = [NOTE] `#[warn(unused_variables)]` on by default

[WARNING] variable does not need to be mutable
 --> src/lib.rs:3:21
  |
3 |                 let mut b = 10;
  |                     ----^
  |                     |
  |                     [HELP] remove this `mut`
  |
  = [NOTE] `#[warn(unused_mut)]` on by default


"#]])
        .run();
    assert_ui().eq(
        p.read_file("src/lib.rs"),
        str![[r#"

            pub fn a() {
                let mut b = 10;
                let _ = b;

                let c = 10;
            }
            
"#]],
    );
}
