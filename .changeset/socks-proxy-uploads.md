---
default: patch
---

# Upload through SOCKS proxies from the environment

Uploads now honor SOCKS proxies configured through `ALL_PROXY` and `all_proxy`
instead of failing with `Connection refused` before connecting. Proxy precedence
and the HTTP proxy fallback when those variables are empty or unset stay the same.
