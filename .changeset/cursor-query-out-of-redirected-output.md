---
default: patch
---

# Keep the cursor query out of redirected output

Running xmtr in a terminal with stdout redirected, as in `xmtr --json host > trace.json`, wrote the escape sequence `ESC [ 6 n` at the start of the file, so the JSON did not parse. The panel now runs only when stdout is on the terminal. With stdout redirected, the line view shows progress on stderr. ([#29](https://github.com/Alice39s/xmtr/pull/29), fixes [#25](https://github.com/Alice39s/xmtr/issues/25))
