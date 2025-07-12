use rustfix::diagnostics::Diagnostic;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum CheckOutput {
    Artifact(Artifact),
    Message(Message),
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct Artifact {
    #[serde(flatten)]
    pub build_unit: BuildUnit,
    pub fresh: bool,
}

#[derive(Deserialize, Debug)]
pub struct Message {
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
