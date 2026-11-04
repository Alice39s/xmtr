---
default: patch
---

# Install the patched tracer core through Cargo

Cargo installations now use `xmtr-trippy-core`, the same patched core as the release binaries. This includes the Windows receive memory fix, Android IPv4 header handling and Unix socket polling for large sets of open descriptors. The core is published before xmtr and shares its version. ([#36](https://github.com/Alice39s/xmtr/pull/36))
