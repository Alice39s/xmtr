---
default: patch
---

# Install with BusyBox wget and OpenWrt uclient-fetch

`install.sh` now passes `--https-only` to wget only when wget supports it. On Alpine, BusyBox and OpenWrt systems without curl, the script stopped with the wget usage text. ([#16](https://github.com/Alice39s/xmtr/pull/16), fixes [#9](https://github.com/Alice39s/xmtr/issues/9))
