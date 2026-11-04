---
default: patch
---

# Finish Windows receives before closing their sockets

Wait for submitted Windows receives to finish cancellation before releasing their event or receive storage. Poll completed receives through their original overlapped state.
