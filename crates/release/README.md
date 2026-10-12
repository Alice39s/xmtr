# xmtr-release

Library behind `xmtr self update` and `xmtr-installer`: it finds the latest [xmtr](https://github.com/Alice39s/xmtr) release, downloads the archive for the target it was compiled for, checks it against the release `SHA256SUMS`, and unpacks the `xmtr` binary.

It is published so that the `xmtr` crate can be installed with `cargo install xmtr`; it has no stable API of its own.

Licensed under either of Apache License 2.0 or MIT, at your option.
