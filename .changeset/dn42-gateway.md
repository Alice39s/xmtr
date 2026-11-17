---
default: minor
---

# Upload and report errors from hosts that reach only DN42

Set `XMTR_API=https://xmtr.ip-api.dn42` to upload through the gateway on DN42; trace links still open on IPMap at xmsl.dev. A gateway under `.dn42` is verified against the DN42 root CA that xmtr now carries, and uploads through it carry no public source address. Error reports go through the DN42 gateway with `--mode dn42` or a `.dn42` API. ([#44](https://github.com/Alice39s/xmtr/pull/44))
