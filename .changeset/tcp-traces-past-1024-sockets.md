---
default: patch
---

# Keep parallel TCP traces running past 1024 open sockets

With about 24 or more TCP routes traced at once to targets that do not answer, xmtr aborted with `fd must be in the range 0..FD_SETSIZE` and printed nothing. The bundled trippy-core now waits on its sockets with `poll`, which has no descriptor limit. ([#15](https://github.com/Alice39s/xmtr/pull/15), fixes [#8](https://github.com/Alice39s/xmtr/issues/8))
