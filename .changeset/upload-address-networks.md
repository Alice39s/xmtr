---
default: patch
---

# Upload the source and destination as their /24 or /48 network

Uploaded routes carried the exact public source address and the exact destination, and a target given as an IP address carried the destination again. They are now cut to their network, the /24 of IPv4 or the /48 of IPv6, such as `203.0.113.0` for `203.0.113.7`, which keeps the place and AS where IPMap draws the ends of the route. `--json` prints the same values; the tables in the terminal still show the full destination.
