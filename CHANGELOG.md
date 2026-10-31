## 0.3.2 (2026-10-31)

### Features

#### Start routes on the map at the address the trace left from

Each uploaded route carries the public address its probes left from, and IPMap starts the route there rather than at the address the upload came from. When the local address is private or otherwise not public, xmtr asks the gateway over a direct HTTPS connection from that local address, ignoring proxy settings; the lookup runs during the trace and waits at most 3 seconds. `--hide-origin` uploads the routes without this address. ([#32](https://github.com/Alice39s/xmtr/pull/32))

### Fixes

#### Keep custom gateway DNS separate from proxy connections

With `--dns`, uploads no longer connect to the gateway's address and port when
opening an HTTP or SOCKS proxy connection. Direct uploads and locally resolving
SOCKS proxies still use the selected gateway resolver. HTTP, SOCKS4a and SOCKS5h
proxies receive the gateway hostname without a local lookup; proxy endpoints use
the system resolver.
([#31](https://github.com/Alice39s/xmtr/pull/31))

#### Upload through SOCKS proxies without hanging

Uploads honor SOCKS proxies configured through `ALL_PROXY` and `all_proxy`.
SOCKS negotiation, authentication, TLS and the upload response share the request
timeout, so a proxy that accepts a connection without answering cannot leave the
upload stuck. Proxy precedence and the HTTP fallback remain unchanged.
([#31](https://github.com/Alice39s/xmtr/pull/31))

## 0.3.1 (2026-10-29)

### Features

#### Choose how progress is shown and match the terminal's colors

`--progress` picks the live view: `panel`, `line` (one status line that is redrawn in place, for small terminals) or `plain` (text lines for logs). The default, `auto`, uses the panel when the terminal is large enough and reports the cursor position, and the line view otherwise. It prints plain lines when stderr is not a terminal or `TERM=dumb`, so redirected stderr now shows each step and finished route.

`--theme` picks the colors. `auto` asks the terminal for its background color and uses RGB colors when `COLORTERM` announces true color. `dark`, `light`, `dark-ansi` and `light-ansi` set the colors directly.

Route tables that do not fit the terminal are printed with one short line per hop. ([#21](https://github.com/Alice39s/xmtr/pull/21))

### Fixes

#### Limit the size of the unpacked executable

The 64 MiB limit on release downloads now also applies to the executable unpacked from the archive, so a small, highly compressed archive can no longer make `self update` or the installer allocate far more memory. ([#7](https://github.com/Alice39s/xmtr/pull/7), fixes [#4](https://github.com/Alice39s/xmtr/issues/4))

#### Survive a closed stdout and print gateway text safely

When the reader of stdout goes away, as in `xmtr --json host | head -1`, xmtr no longer aborts; if the upload succeeded, the IPMap link is printed on stderr instead. Control characters in the trace ID, link and error text from the gateway are replaced with U+FFFD, so a gateway reply cannot send escape sequences to the terminal. ([#19](https://github.com/Alice39s/xmtr/pull/19), fixes [#12](https://github.com/Alice39s/xmtr/issues/12))

#### Keep the cursor query out of redirected output

Running xmtr in a terminal with stdout redirected, as in `xmtr --json host > trace.json`, wrote the escape sequence `ESC [ 6 n` at the start of the file, so the JSON did not parse. The panel now runs only when stdout is on the terminal. With stdout redirected, the line view shows progress on stderr. ([#29](https://github.com/Alice39s/xmtr/pull/29), fixes [#25](https://github.com/Alice39s/xmtr/issues/25))

#### Install with BusyBox wget and OpenWrt uclient-fetch

`install.sh` now passes `--https-only` to wget only when wget supports it. On Alpine, BusyBox and OpenWrt systems without curl, the script stopped with the wget usage text. ([#16](https://github.com/Alice39s/xmtr/pull/16), fixes [#9](https://github.com/Alice39s/xmtr/issues/9))

#### Upload to an IPv6 gateway address with a custom resolver

`--api 'http://[::1]:8080'` together with `--dns` failed with `Label contains invalid characters`, because the brackets reached the resolver. IPv6 literals now skip DNS like IPv4 literals do. ([#6](https://github.com/Alice39s/xmtr/pull/6), fixes [#3](https://github.com/Alice39s/xmtr/issues/3))

#### Accept very large time limits

A very large `--time-limit`, such as `18446744073709551615s`, overflowed the round budget: debug builds panicked and release builds could stop after one round. The budget is now capped by `--max-rounds` for every accepted duration. ([#5](https://github.com/Alice39s/xmtr/pull/5), fixes [#2](https://github.com/Alice39s/xmtr/issues/2))

#### Keep route tables readable in narrow terminals

When stdout was a terminal too narrow for the route table, every column wrapped into a few characters, with `Hos` and `t` on separate lines. Now only long host names wrap. A table that still does not fit is printed as one line per hop, with the host, the average round trip and any loss. ([#27](https://github.com/Alice39s/xmtr/pull/27), fixes [#23](https://github.com/Alice39s/xmtr/issues/23))

#### Refuse release archives that do not contain an executable

`xmtr self update` and the installer now reject an archive whose `xmtr` entry is not a regular file or does not start with the executable header of the platform. Before, a directory, symlink or empty entry replaced the installed binary with an empty file and the update reported success. ([#17](https://github.com/Alice39s/xmtr/pull/17), fixes [#10](https://github.com/Alice39s/xmtr/issues/10))

#### Build release packages with the xmtr-ci project

The release workflow now runs through `scripts/ci`, one Python project that reads every target from a single table. The packages are the same apart from two metadata fields: `<packageSourceUrl>` in the Chocolatey nuspec points at `scripts/ci/src/xmtr_ci/packages/chocolatey.py`, and `Generator` in the wheel `WHEEL` file is `xmtr-ci`. ([#1](https://github.com/Alice39s/xmtr/pull/1))

#### Show the short trace link in the README

Uploads return `https://xmsl.dev/xmtr/<trace ID>`, a short link that shows a preview when shared and opens the trace on IPMap. The README sample output and demo link now use it. ([#22](https://github.com/Alice39s/xmtr/pull/22))

#### Keep parallel TCP traces running past 1024 open sockets

With about 24 or more TCP routes traced at once to targets that do not answer, xmtr aborted with `fd must be in the range 0..FD_SETSIZE` and printed nothing. The bundled trippy-core now waits on its sockets with `poll`, which has no descriptor limit. ([#15](https://github.com/Alice39s/xmtr/pull/15), fixes [#8](https://github.com/Alice39s/xmtr/issues/8))

#### Reject a time limit that fits no round

`--time-limit` must now be longer than `--interval`. A round ends after at least one interval, so shorter limits returned no hops and reported reachable targets as `no probe was answered`; such runs now stop with an argument error. ([#18](https://github.com/Alice39s/xmtr/pull/18), fixes [#11](https://github.com/Alice39s/xmtr/issues/11))

## 0.3.0 (2026-10-14)

### Features

#### Show progress and results in a terminal panel

In a terminal, xmtr now draws a panel below the prompt while it runs. Numbered steps (1. Resolve, 2. Trace, 3. Lookup, 4. Upload, 5. Result) show which step is active, each route has a progress bar, reverse DNS and AS lookups show how many addresses have answered, and the upload shows that it is still waiting for the gateway.

When the run ends, the panel turns into the result: the IPMap link first, then notes about skipped targets and failed routes, then each route as a table. Runs of three or more silent hops fold into one line, and long host names are shortened before their addresses.

Ctrl-C stops a run and exits with status 130. Piped or redirected output, `--quiet`, and terminals that do not report the cursor position keep the plain text output.
