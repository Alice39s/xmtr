---
default: patch
---

# Accept very large time limits

A very large `--time-limit`, such as `18446744073709551615s`, overflowed the round budget: debug builds panicked and release builds could stop after one round. The budget is now capped by `--max-rounds` for every accepted duration. ([#5](https://github.com/Alice39s/xmtr/pull/5), fixes [#2](https://github.com/Alice39s/xmtr/issues/2))
