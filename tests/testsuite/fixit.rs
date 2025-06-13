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
src/lib.rs: 1 fixes

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
src/lib.rs: 1 fixes
[WARNING] unused variable: `c`
 --> src/lib.rs:6:21
  |
6 |                 let c = 10;
  |                     ^ [HELP] if this is intentional, prefix it with an underscore: `_c`
  |
  = [NOTE] `#[warn(unused_variables)]` on by default


"#]])
        .run();
    assert_ui().eq(
        p.read_file("src/lib.rs"),
        str![[r#"

            pub fn a() {
                let b = 10;
                let _ = b;

                let c = 10;
            }
            
"#]],
    );
}
