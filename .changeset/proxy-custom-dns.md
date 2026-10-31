---
default: patch
---

# Keep custom gateway DNS separate from proxy connections

With `--dns`, uploads no longer connect to the gateway's address and port when
opening an HTTP or SOCKS proxy connection. Direct uploads and locally resolving
SOCKS proxies still use the selected gateway resolver. HTTP, SOCKS4a and SOCKS5h
proxies receive the gateway hostname without a local lookup; proxy endpoints use
the system resolver.
([#31](https://github.com/Alice39s/xmtr/pull/31))
