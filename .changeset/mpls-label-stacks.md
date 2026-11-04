---
default: minor
---

# Show and upload the MPLS label stacks that routers quote

A router that forwards a probe inside an MPLS tunnel can quote the tunnel's label stack in its ICMP reply (RFC 4950). xmtr now reads these extensions. The result table lists each label under the router's address, such as `[MPLS: Lbl 24001 TC 0 S 1 TTL 1]`, the form `mtr -e` prints, and the upload carries the stack with the address, so IPMap knows which hops answered from inside a tunnel. ([#36](https://github.com/Alice39s/xmtr/pull/36))
