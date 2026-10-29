---
default: patch
---

# Keep route tables readable in narrow terminals

When stdout was a terminal too narrow for the route table, every column wrapped into a few characters, with `Hos` and `t` on separate lines. Now only long host names wrap. A table that still does not fit is printed as one line per hop, with the host, the average round trip and any loss. ([#27](https://github.com/Alice39s/xmtr/pull/27), fixes [#23](https://github.com/Alice39s/xmtr/issues/23))
