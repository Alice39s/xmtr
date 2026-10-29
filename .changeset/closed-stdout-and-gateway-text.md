---
default: patch
---

# Survive a closed stdout and print gateway text safely

When the reader of stdout goes away, as in `xmtr --json host | head -1`, xmtr no longer aborts; if the upload succeeded, the IPMap link is printed on stderr instead. Control characters in the trace ID, link and error text from the gateway are replaced with U+FFFD, so a gateway reply cannot send escape sequences to the terminal. ([#19](https://github.com/Alice39s/xmtr/pull/19), fixes [#12](https://github.com/Alice39s/xmtr/issues/12))
