---
default: patch
---

# Keep each round of a UDP trace on one path

Classic UDP probes change their destination port with every probe, so a router that spreads flows over parallel links can send each hop's probe down a different link. UDP over raw sockets now uses Dublin traceroute, keeping both ports fixed within each round and matching replies even when NAT changes the UDP checksum. ICMP, TCP and UDP over unprivileged sockets use classic probes. ([#36](https://github.com/Alice39s/xmtr/pull/36))
