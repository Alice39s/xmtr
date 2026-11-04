---
default: patch
---

# Stop Windows traces from overwriting memory when a reply arrives

On Windows, each reply made the system write the receive flags and the length of the source address into stack memory that the tracer no longer owned, overwriting whatever the tracer kept there at that moment. Depending on how a build was laid out, this went unnoticed or crashed the trace with an access violation. Both values now live on the heap. ([#36](https://github.com/Alice39s/xmtr/pull/36))
