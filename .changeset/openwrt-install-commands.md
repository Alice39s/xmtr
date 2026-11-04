---
default: patch
---

# Install OpenWrt packages with chained setup commands

The OpenWrt guide uses `opkg-key add` to trust the opkg feed key and chains the download, repository setup and installation steps with `&&`. The APK v3 commands select the repository with `apk --print-arch` and refresh the index with `apk -U add xmtr`.
