# xmtr-trippy-core

The network tracing core used by [xmtr](https://github.com/Alice39s/xmtr), based on trippy-core 0.13.0 under the Apache-2.0 license.

This package includes xmtr's platform fixes:

- Use Linux IPv4 header handling on Android.
- Wait for Unix sockets with `poll`, including descriptors above `FD_SETSIZE`.
- Keep the buffers used by Windows overlapped receives at stable heap addresses.

The package follows xmtr's version and is published before the xmtr binary crate. Cargo installations and prebuilt binaries use this core. The upstream API remains available under the `trippy_core` library name.
