---
default: patch
---

# Reject a time limit that fits no round

`--time-limit` must now be longer than `--interval`. A round ends after at least one interval, so shorter limits returned no hops and reported reachable targets as `no probe was answered`; such runs now stop with an argument error. ([#18](https://github.com/Alice39s/xmtr/pull/18), fixes [#11](https://github.com/Alice39s/xmtr/issues/11))
