<h1 align="center">xmtr</h1>

<p align="center">
  Trace several targets at once and see every hop on a map.
</p>

<p align="center">
  <a href="https://github.com/Alice39s/xmtr/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/Alice39s/xmtr?style=flat-square"></a>
  <a href="https://github.com/Alice39s/xmtr/actions/workflows/release.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/Alice39s/xmtr/release.yml?branch=main&style=flat-square"></a>
  <img alt="License" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square">
</p>

`xmtr` runs concurrent traceroutes with [trippy](https://github.com/fujiapple852/trippy), stops each route as soon as its path is stable, and uploads the result to [xmsl.dev IPMap](https://xmsl.dev/ipmap), which places every hop on a map and replays the route.

xmtr prints each route as a table, then a trace ID and a link that opens the route on IPMap.

## Install

**macOS, Linux, FreeBSD, Termux**

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | sudo sh
```

With `sudo` the binary goes to `/usr/local/bin`, where `sudo xmtr` finds it. Without `sudo` it goes to `~/.local/bin`; on Linux, grant raw sockets once with `setcap` (see [Privileges](#privileges)) and run it as your user. On Termux, leave out `sudo`.

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/Alice39s/xmtr/main/install.ps1 | iex
```

The script picks the build for your system, checks it against the release `SHA256SUMS`, and runs a small installer that places a single `xmtr` binary and then deletes itself. Nothing else is written, except the user `PATH` entry on Windows.

| Default location | |
| --- | --- |
| Linux, macOS, FreeBSD | `~/.local/bin`, or `/usr/local/bin` as root |
| Termux | `$PREFIX/bin` |
| Windows | `%LOCALAPPDATA%\Programs\xmtr` |

Pass installer options after `sh -s --`:

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | sh -s -- --dir ~/bin --version v0.2.0
```

`XMTR_INSTALL_DIR` sets the directory, and `XMTR_RELEASES` points both the script and the installer at a mirror of the releases page. Archives for manual installs are on the [releases page](https://github.com/Alice39s/xmtr/releases).

**Update or remove**

```sh
xmtr self update        # install the latest release over this binary
xmtr self uninstall     # delete this binary
```

## Platforms

All builds are cross-compiled on Linux.

| | System | Architectures |
| :-: | --- | --- |
| <img src="https://cdn.simpleicons.org/linux" width="20" alt=""> | Linux (glibc and musl) | x86_64, i686, aarch64, armv7, armv6, riscv64, loongarch64 |
| <img src="https://cdn.simpleicons.org/linux" width="20" alt=""> | Linux (glibc) | powerpc64le, powerpc64, s390x, sparc64 |
| <img src="https://cdn.simpleicons.org/apple" width="20" alt=""> | macOS | Apple silicon, x86_64 |
| <img src="https://cdn.jsdelivr.net/gh/devicons/devicon/icons/windows11/windows11-original.svg" width="20" alt=""> | Windows 10+ | x86_64, i686, ARM64 |
| <img src="https://cdn.simpleicons.org/freebsd" width="20" alt=""> | FreeBSD | x86_64 |
| <img src="https://cdn.simpleicons.org/android" width="20" alt=""> | Android (Termux, root) | aarch64, armv7, x86_64, i686 |

Not available:

| | System | Reason |
| :-: | --- | --- |
| <img src="https://cdn.simpleicons.org/netbsd" width="20" alt=""> | NetBSD | trippy's socket layer does not compile there |
| <img src="https://cdn.simpleicons.org/openbsd" width="20" alt=""> | OpenBSD, DragonFly BSD | Rust ships no prebuilt standard library for cross builds |
| | illumos, Solaris | trippy's socket layer does not compile there |
| | iOS, WebAssembly | no raw socket access for command-line programs |

## Privileges

Tracing needs raw sockets.

| System | Run as |
| --- | --- |
| Linux | root, or once: `sudo setcap cap_net_raw+ep $(command -v xmtr)` |
| macOS | any user; ICMP and UDP use unprivileged sockets, TCP needs root |
| Windows | an Administrator terminal |
| FreeBSD, Android | root |

`xmtr self update` replaces the binary, so run `setcap` again after an update.

## How a run works

1. Every target is resolved. A hostname with both A and AAAA records is traced over IPv4 and IPv6 at the same time; a family without a local route is skipped.
2. Up to 8 routes run in parallel (`-j`). Each tracer has its own ICMP identifier, UDP source port and sequence range, so replies never mix between routes.
3. A route stops once it has run `--min-rounds` rounds, no new hop has appeared for 3 rounds, and the target answered. A route that never reaches its target gets 5 extra rounds. `--max-rounds` and `--time-limit` are hard limits.
4. With `--protocol auto`, a target that does not answer ICMP is traced again over TCP port 443, or UDP with unprivileged sockets. Both routes are uploaded.
5. Hop addresses get reverse DNS names and origin AS numbers ([Team Cymru](https://www.team-cymru.com/ip-asn-mapping)).
6. The routes are uploaded to `--api`. The response holds the trace ID, an 8-letter key and the IPMap link. Pass `--no-upload` to keep everything local, or `--hide-origin` to leave out the public address the upload came from.

## DNS

`--dns` selects the resolver for target names, the upload, reverse DNS and AS lookups.

| Value | Resolver |
| --- | --- |
| `auto` | system configuration; target names go through the OS, so hosts files apply |
| `cloudflare`, `google`, `quad9`, `9.9.9.9` | plain DNS over UDP and TCP |
| `tcp://[2620:fe::fe]` | DNS over TCP |
| `tls://[2606:4700:4700::1111]`, `dot://dns.quad9.net` | DNS over TLS |
| `https://cloudflare`, `doh://dns.google/dns-query` | DNS over HTTPS |
| `quic://dns.adguard-dns.com` | DNS over QUIC |

`cloudflare`, `google` and `quad9` also work as the host of a URL and include both IPv4 and IPv6 servers. A hostname in a URL is looked up once with the system resolver; certificates are checked against public roots. DNS over HTTPS needs a hostname or an IPv4 address.

## Options

```text
Usage: xmtr [OPTIONS] <TARGET>...
       xmtr self <update|uninstall>

-p, --protocol <auto|icmp|udp|tcp>   probe protocol (default auto)
-4, --ipv4 / -6, --ipv6              trace one family only
    --min-rounds <N>                 rounds before a route may stop (default 5)
-c, --max-rounds <N>                 hard limit on rounds (default 30)
-t, --time-limit <DURATION>          time limit per route (default 45s)
-i, --interval <DURATION>            minimum time between rounds (default 1s)
-m, --max-ttl <N>                    maximum TTL (default 30)
    --port <PORT>                    TCP destination port (default 443)
-j, --parallel <N>                   routes traced at the same time
-u, --unprivileged                   unprivileged sockets (macOS)
    --dns <RESOLVER>                 see DNS above (default auto)
    --no-rdns                        skip reverse DNS and AS lookups
    --no-upload                      print the routes only
    --hide-origin                    do not attach the sender address
    --api <URL>                      upload gateway, also XMTR_API
    --json                           print the upload payload
-q, --quiet                          print only the IPMap link
```

## Build from source

```sh
cargo build --release --workspace
```

The workspace builds `xmtr` and `xmtr-installer`. Release builds use [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) for Linux, Windows and macOS, [cross](https://github.com/cross-rs/cross) for FreeBSD, powerpc64 and sparc64, and [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) for Android; see `.github/workflows/release.yml`.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.
