use rustfix::diagnostics::Diagnostic;
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
#[serde(untagged)]
pub enum CheckOutput {
    Artifact(Artifact),
    Message(Message),
}

impl CheckOutput {
    pub fn build_unit(&self) -> Option<&BuildUnit> {
        match self {
            Self::Artifact(a) => Some(&a.build_unit),
            Self::Message(m) => Some(&m.build_unit),
        }
    }
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
#[allow(dead_code)]
pub struct Artifact {
    #[serde(flatten)]
    pub build_unit: BuildUnit,
    pub fresh: bool,
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct Message {
    #[serde(flatten)]
    pub build_unit: BuildUnit,
    pub message: MessageDiagnostic,
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq)]
pub struct MessageDiagnostic {
    #[serde(flatten)]
    pub level: DiagnosticLevel,
    #[serde(flatten)]
    pub diagnostic: Diagnostic,
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "level", rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Error,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuildUnit {
    pub package_id: String,
    pub target: Target,
}

#[derive(Deserialize, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
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

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug, PartialOrd, Ord)]
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

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug, PartialOrd, Ord)]
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
