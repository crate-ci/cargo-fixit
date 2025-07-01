#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![warn(clippy::print_stderr)]
#![warn(clippy::print_stdout)]

mod check;
mod errors;
mod flags;
pub mod shell;
mod vcs;

pub use check::*;
pub use errors::*;
pub use flags::CheckFlags;
pub use vcs::*;

#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;
