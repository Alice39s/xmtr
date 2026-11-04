---
default: patch
---

# Package the binary source and root documentation for Cargo

Cargo source archives select `src/`, the root README and the two license files. Packaging after tests now uses the same inputs as a clean checkout, including when local tools have created other README files. ([#36](https://github.com/Alice39s/xmtr/pull/36))
