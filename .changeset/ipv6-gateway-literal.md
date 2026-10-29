---
default: patch
---

# Upload to an IPv6 gateway address with a custom resolver

`--api 'http://[::1]:8080'` together with `--dns` failed with `Label contains invalid characters`, because the brackets reached the resolver. IPv6 literals now skip DNS like IPv4 literals do. ([#6](https://github.com/Alice39s/xmtr/pull/6), fixes [#3](https://github.com/Alice39s/xmtr/issues/3))
