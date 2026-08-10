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
    pub message: MessageDiagnostic,
}

#[derive(Deserialize, Debug)]
pub struct MessageDiagnostic {
    #[serde(flatten)]
    pub level: DiagnosticLevel,
    #[serde(flatten)]
    pub diagnostic: Diagnostic,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
#[serde(tag = "level", rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Error,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
pub struct BuildUnit {
    pub package_id: String,
    pub target: Target,
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
pub struct Target {
    pub kind: Vec<TargetKind>,
    pub crate_types: Vec<CrateType>,
    pub name: String,
    pub src_path: String,
    pub edition: String,
    pub doc: bool,
    pub doctest: bool,
    pub test: bool,
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub enum TargetKind {
    Bin,
    Test,
    Bench,
    Example,
    CustomBuild,
    #[serde(untagged)]
    Lib(CrateType),
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
