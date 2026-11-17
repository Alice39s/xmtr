---
default: minor
---

# Trace DN42 routes with local IPDB annotations

Select `--mode dn42` to read hop ASNs and host-specific PTR names from DN42 IPDB.
Use `--dn42-refresh` to download a verified database cache or `--dn42-db` to open
an extracted release bundle. DN42 traces stay local; the default internet mode
continues to use DNS annotations and IPMap uploads.

Building from source requires Rust 1.89 or later for the DN42 SDK.
