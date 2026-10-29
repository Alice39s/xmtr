# Vendored crates

## trippy-core

`trippy-core/` is [trippy-core 0.13.0](https://crates.io/crates/trippy-core/0.13.0) from crates.io with two changes, and `Cargo.toml` uses it through `[patch.crates-io]`. `LICENSE` is the Apache-2.0 license from the [trippy repository](https://github.com/fujiapple852/trippy).

The first change makes Android use the Linux code path for the IPv4 header byte order. In 0.13.0 Android takes the macOS and BSD path. That path sets `IP_HDRINCL` on an ICMP datagram socket, which the Linux kernel rejects, so every IPv4 trace on Android fails with `set header included: Protocol not available`. The change is in `src/net/platform/unix.rs` and `src/net/platform/byte_order.rs`. It matches [fujiapple852/trippy#1896](https://github.com/fujiapple852/trippy/pull/1896).

The second change makes the Unix sockets wait with `poll` instead of `select`. A TCP tracer opens one socket per probe, so about 24 parallel TCP routes to targets that do not answer push descriptor numbers past `FD_SETSIZE` (1024), and `select` then aborts the process with `fd must be in the range 0..FD_SETSIZE`. `poll` has no such limit. The change is in `src/net/platform/unix.rs` and the `IoOperation` name in `src/error.rs`. Upstream tracks the problem in [fujiapple852/trippy#1790](https://github.com/fujiapple852/trippy/issues/1790).

Remove this directory and the `[patch.crates-io]` entry once a trippy-core release includes both fixes.

`cargo install xmtr` builds from crates.io and ignores `[patch.crates-io]`. Those builds use trippy-core 0.13.0 as published. The release binaries, npm, PyPI, Homebrew and the other channels all ship the patched build.
