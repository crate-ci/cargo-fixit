# cargo-fixit

> Prototype for alternative `cargo fix` ([rust-lang/cargo#13214](https://github.com/rust-lang/cargo/issues/13214))

[![Documentation](https://img.shields.io/badge/docs-master-blue.svg)][Documentation]
![License](https://img.shields.io/crates/l/cargo-fixit.svg)
[![Crates Status](https://img.shields.io/crates/v/cargo-fixit.svg)][Crates.io]

This is meant to be a drop-in replacement for `cargo fix`, except faster.

Before
```console
$ cargo fix
$ cargo clippy --fix
```
After
```console
$ cargo install cargo-fixit
$ cargo fixit
$ cargo fixit --clippy
```

Expectations
- Edition migration is unsupported
- The CLI is modeled off of `cargo fix` 1.89 (no implicit `--all-targets`)

## Opt-in target parallelism

Executable targets with independent sources and build inputs can be fixed in the same pass:

```console
$ cargo fixit --Zassume-independent-targets
```

This optimization is disabled by default. `include!`, `#[path]`, build scripts,
and procedural macros can make targets depend on each other's sources. Successful
compilation cannot prove semantic equivalence, so only opt in when those inputs
are independent.

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/license/mit>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual-licensed as above, without any additional terms or
conditions.

[Crates.io]: https://crates.io/crates/cargo-fixit
[Documentation]: https://docs.rs/cargo-fixit
