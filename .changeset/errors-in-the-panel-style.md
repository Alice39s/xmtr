---
default: minor
---

# Show errors in the style of the result panel

On a terminal, an error that ends a run, such as missing raw socket rights or no target left to trace, and a failed upload appear in a framed panel with the colors of the result panel. Skipped targets and failed routes printed beside the route tables get the icons and colors they have in the panel. Logs, pipes, `--quiet` and `--progress plain` keep the plain `error:` lines, and exit statuses stay the same.
