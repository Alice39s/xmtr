---
default: minor
---

# Start routes on the map at the address the trace left from

Each uploaded route carries the public address its probes left from, and IPMap starts the route there rather than at the address the upload came from. When the local address is private or otherwise not public, xmtr asks the gateway over a direct HTTPS connection from that local address, ignoring proxy settings; the lookup runs during the trace and waits at most 3 seconds. `--hide-origin` uploads the routes without this address. ([#32](https://github.com/Alice39s/xmtr/pull/32))
