#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(clippy::print_stderr)]
#![warn(clippy::print_stdout)]

pub mod core;
mod errors;
pub mod ops;
pub mod util;

pub use errors::*;

#[doc = include_str!("../README.md")]
#[cfg(doctest)]
pub struct ReadmeDoctests;
