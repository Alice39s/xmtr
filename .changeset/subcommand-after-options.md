---
default: patch
---

# Stop with an error when options come before a subcommand

Options before `traces` or `self` made the subcommand and its arguments targets: `xmtr --dns quad9 traces delete <TRACE>` traced hosts named `traces` and `delete` and uploaded the routes. xmtr now exits with status 2 before it resolves or traces anything and asks for the options after the subcommand, such as `xmtr traces delete --dns quad9 <TRACE>`. ([#41](https://github.com/Alice39s/xmtr/pull/41))
