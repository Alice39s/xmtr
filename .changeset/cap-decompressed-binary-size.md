---
default: patch
---

# Limit the size of the unpacked executable

The 64 MiB limit on release downloads now also applies to the executable unpacked from the archive, so a small, highly compressed archive can no longer make `self update` or the installer allocate far more memory. ([#7](https://github.com/Alice39s/xmtr/pull/7), fixes [#4](https://github.com/Alice39s/xmtr/issues/4))
