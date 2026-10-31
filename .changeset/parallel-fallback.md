---
default: minor
---

# Start the fallback protocol beside ICMP after 4 rounds

With `--protocol auto`, a target that ICMP has not reached after 4 rounds is also traced over TCP port 80, or over UDP with unprivileged sockets, while the ICMP trace goes on. Before, the fallback started only once the ICMP route had finished. An ICMP route that ends sooner without the target starts the fallback when it ends. The fallback route gets its own row under the ICMP row in the panel, line and plain views, counts toward `-j`, and is uploaded next to the ICMP route. `--port` now sets the port of `--protocol tcp` only.
