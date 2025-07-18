# cargo-fixit

> Protoype for alternative `cargo fix` ([rust-lang/cargo#13214](https://github.com/rust-lang/cargo/issues/13214))

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

## License

Licensed under either of

* Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
* MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual-licensed as above, without any additional terms or
conditions.

[Crates.io]: https://crates.io/crates/cargo-fixit
[Documentation]: https://docs.rs/cargo-fixit
