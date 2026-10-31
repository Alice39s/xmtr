---
default: minor
---

# Offer the latest release after a run

After a run in a terminal, xmtr looks up the latest release, waiting at most 3 seconds. When it is newer, xmtr asks whether to install it and then whether to run the same command again with the new version; Enter answers yes to both. A binary owned by a package manager, or in a directory xmtr cannot write to, gets the command that updates it instead. Runs with `--json`, `--quiet` or `--no-upload`, and runs without a terminal on stdin and stderr, skip the lookup. ([#34](https://github.com/Alice39s/xmtr/pull/34))
