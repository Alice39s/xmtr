---
default: patch
---

# Keep each round of a UDP trace on one path

Classic UDP probes change their destination port with every probe, so a router that spreads flows over parallel links could send each hop's probe down a different link, and one round mixed hops of several paths. With raw sockets, UDP traces now use Paris traceroute: every probe of a round has the same ports and UDP checksum. UDP over unprivileged sockets cannot set the checksum and keeps the classic probes. ([#36](https://github.com/Alice39s/xmtr/pull/36))
