---
default: patch
---

# Upload through SOCKS proxies without hanging

Uploads honor SOCKS proxies configured through `ALL_PROXY` and `all_proxy`.
SOCKS negotiation, authentication, TLS and the upload response share the request
timeout, so a proxy that accepts a connection without answering cannot leave the
upload stuck. Proxy precedence and the HTTP fallback remain unchanged.
([#31](https://github.com/Alice39s/xmtr/pull/31))
