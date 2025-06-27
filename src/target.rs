use serde::Deserialize;

#[derive(Deserialize, Hash, PartialEq, Clone)]
pub struct Target {
    // All fields, tests
    kind: Vec<Kind>,
    crate_types: Vec<CrateType>,
    name: String,
    src_path: String,
    edition: String,
    doc: bool,
    doctest: bool,
    test: bool,
}
impl Eq for Target {}

#[derive(Deserialize, Hash, PartialEq, Clone)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub enum Kind {
    Bin,
    Example,
    Test,
    Bench,
    CustomBuild,
    Lib,
    Rlib,
    Dylib,
    Cdylib,
    Staticlib,
    ProcMacro,
    #[serde(untagged)]
    Other(String),
}
impl Eq for Kind {}

#[derive(Deserialize, Hash, PartialEq, Clone)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub enum CrateType {
    Bin,
    Lib,
    Rlib,
    Dylib,
    Cdylib,
    Staticlib,
    ProcMacro,
    #[serde(untagged)]
    Other(String),
}
impl Eq for CrateType {}
