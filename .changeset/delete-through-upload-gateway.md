---
default: patch
---

# Delete commands name the gateway the upload went through

The printed delete command used the address the gateway reported for itself, which hosts that upload through the DN42 gateway or another proxy cannot reach. It now names the `--api` gateway the upload used, so it runs where the upload ran. ([#45](https://github.com/Alice39s/xmtr/pull/45))
