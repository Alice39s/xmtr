#!/bin/sh
# Grants raw sockets to /usr/bin/xmtr, so every user can trace without root, as the mtr packages do.
if command -v setcap >/dev/null 2>&1 && setcap cap_net_raw+ep /usr/bin/xmtr; then
  exit 0
fi
echo "xmtr: could not grant raw sockets with setcap; run xmtr as root to trace" >&2
