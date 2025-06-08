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
        .with_status(1)
        .with_stderr_data(str![[r#"
Error: not implemented

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
