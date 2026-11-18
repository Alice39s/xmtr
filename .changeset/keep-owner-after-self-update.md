---
default: patch
---

# Keep the owner of the binary after an update through sudo

`sudo xmtr self update` gave the new binary to root, and the installer run through `sudo` over an existing binary also gave it root's umask. A binary that belonged to another account, such as the Termux app with mode 700, could then no longer be run by that account. On Unix the new binary now keeps the mode of the one it replaces, and an update or reinstall run as root also keeps its owner and group. ([#46](https://github.com/Alice39s/xmtr/pull/46), fixes [#42](https://github.com/Alice39s/xmtr/issues/42))
