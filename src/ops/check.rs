use std::hash::Hash;

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

#[derive(Deserialize, Eq, Clone, Debug)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub enum Kind {
    CustomBuild,
    #[serde(untagged)]
    Lib(LibKind),
    #[serde(untagged)]
    Bin(BinKind),
    #[serde(untagged)]
    Other(String),
}

impl PartialEq for Kind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Kind::Other(a), Kind::Other(b)) => a == b,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

impl Hash for Kind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::CustomBuild => state.write_u8(1),
            Self::Lib(l) => {
                state.write_u8(2);
                l.hash(state);
            }
            Self::Bin(b) => {
                state.write_u8(3);
                b.hash(state);
            }
            Self::Other(a) => {
                state.write_u8(4);
                a.hash(state);
            }
        };
    }
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub enum LibKind {
    Lib,
    Rlib,
    Dylib,
    Cdylib,
    Staticlib,
    ProcMacro,
}

#[derive(Deserialize, Hash, PartialEq, Clone, Eq, Debug)]
#[serde(rename_all(deserialize = "kebab-case"))]
pub enum BinKind {
    Bin,
    Example,
    Test,
    Bench,
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
