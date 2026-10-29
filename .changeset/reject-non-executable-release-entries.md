---
default: patch
---

# Refuse release archives that do not contain an executable

`xmtr self update` and the installer now reject an archive whose `xmtr` entry is not a regular file or does not start with the executable header of the platform. Before, a directory, symlink or empty entry replaced the installed binary with an empty file and the update reported success. ([#17](https://github.com/Alice39s/xmtr/pull/17), fixes [#10](https://github.com/Alice39s/xmtr/issues/10))
