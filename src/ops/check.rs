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

impl BuildUnit {
    /// Returns whether this is a binary-shaped bin, example, test, or benchmark target.
    pub(crate) fn is_executable_leaf(&self) -> bool {
        matches!(
            self.target.kind.as_slice(),
            [Kind::Bin | Kind::Example | Kind::Test | Kind::Bench]
        ) && matches!(self.target.crate_types.as_slice(), [CrateType::Bin])
    }
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
