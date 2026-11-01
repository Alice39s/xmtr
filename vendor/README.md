# Vendored crates

## trippy-core

`trippy-core/` is [trippy-core 0.13.0](https://crates.io/crates/trippy-core/0.13.0) from crates.io with three changes, and `Cargo.toml` uses it through `[patch.crates-io]`. `LICENSE` is the Apache-2.0 license from the [trippy repository](https://github.com/fujiapple852/trippy).

The first change makes Android use the Linux code path for the IPv4 header byte order. In 0.13.0 Android takes the macOS and BSD path. That path sets `IP_HDRINCL` on an ICMP datagram socket, which the Linux kernel rejects, so every IPv4 trace on Android fails with `set header included: Protocol not available`. The change is in `src/net/platform/unix.rs` and `src/net/platform/byte_order.rs`. It matches [fujiapple852/trippy#1896](https://github.com/fujiapple852/trippy/pull/1896).

The second change makes the Unix sockets wait with `poll` instead of `select`. A TCP tracer opens one socket per probe, so about 24 parallel TCP routes to targets that do not answer push descriptor numbers past `FD_SETSIZE` (1024), and `select` then aborts the process with `fd must be in the range 0..FD_SETSIZE`. `poll` has no such limit. The change is in `src/net/platform/unix.rs` and the `IoOperation` name in `src/error.rs`. Upstream tracks the problem in [fujiapple852/trippy#1790](https://github.com/fujiapple852/trippy/issues/1790).

The third change keeps every value that Windows writes when an overlapped `WSARecvFrom` completes on the heap. In 0.13.0 the receive flags point at a temporary, and the source address length at a field of the socket, which moves after `new_recv_socket_ipv4` and `new_recv_socket_ipv6` post the first receive. When a reply arrives, Windows writes both into stack memory that other code then uses, and an x86_64 build whose tracer thread happened to keep a pointer there crashed with an access violation. The change is in `src/net/platform/windows.rs`. Upstream keeps the flags in the socket since [fujiapple852/trippy#1793](https://github.com/fujiapple852/trippy/issues/1793), which is not in a release yet.

Remove this directory and the `[patch.crates-io]` entry once a trippy-core release includes all three fixes.

`cargo install xmtr` builds from crates.io and ignores `[patch.crates-io]`. Those builds use trippy-core 0.13.0 as published. The release binaries, npm, PyPI, Homebrew and the other channels all ship the patched build.
