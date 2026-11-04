---
default: patch
---

# Report completed Windows receive errors

Return completed receive errors to the tracer instead of polling them as unfinished operations. Only an incomplete Windows overlapped operation is retried. ([#38](https://github.com/Alice39s/xmtr/pull/38))
