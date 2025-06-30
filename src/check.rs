use rustfix::diagnostics::Diagnostic;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CheckMessage {
    pub message: Diagnostic,
    pub package_id: String,
}
