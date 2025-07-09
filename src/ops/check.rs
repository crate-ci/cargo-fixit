use rustfix::diagnostics::Diagnostic;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct CheckMessage {
    #[serde(flatten)]
    pub build_unit: BuildUnit,
    pub message: Diagnostic,
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
pub struct BuildUnit {
    pub package_id: String,
    pub target: Target,
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
pub struct Target {
    kind: Vec<Kind>,
    crate_types: Vec<CrateType>,
    name: String,
    src_path: String,
    edition: String,
    doc: bool,
    doctest: bool,
    test: bool,
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
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

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
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
