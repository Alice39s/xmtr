---
default: patch
---

# Build release packages with the xmtr-ci project

The release workflow now runs through `scripts/ci`, one Python project that reads every target from a single table. The packages are the same apart from two metadata fields: `<packageSourceUrl>` in the Chocolatey nuspec points at `scripts/ci/src/xmtr_ci/packages/chocolatey.py`, and `Generator` in the wheel `WHEEL` file is `xmtr-ci`. ([#1](https://github.com/Alice39s/xmtr/pull/1))
