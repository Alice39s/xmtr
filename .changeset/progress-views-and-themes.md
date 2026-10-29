---
default: minor
---

# Choose how progress is shown and match the terminal's colors

`--progress` picks the live view: `panel`, `line` (one status line that is redrawn in place, for small terminals) or `plain` (text lines for logs). The default, `auto`, uses the panel when the terminal is large enough and reports the cursor position, and the line view otherwise. It prints plain lines when stderr is not a terminal or `TERM=dumb`, so redirected stderr now shows each step and finished route.

`--theme` picks the colors. `auto` asks the terminal for its background color and uses RGB colors when `COLORTERM` announces true color. `dark`, `light`, `dark-ansi` and `light-ansi` set the colors directly.

Route tables that do not fit the terminal are printed with one short line per hop. ([#21](https://github.com/Alice39s/xmtr/pull/21))
