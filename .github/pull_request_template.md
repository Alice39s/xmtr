## Summary

<!-- What changes and why. Link the issue: "Closes #N." -->

## Change file

<!--
Required when users see the change: the xmtr binary, the installer, install.sh, install.ps1,
the published packages or the README instructions. Add one file per change under .changeset/
(knope document-change, or by hand as shown in AGENTS.md) and link this pull request in it.
-->

- [ ] `.changeset/<name>.md` added, with a link to this pull request and the issue
- [ ] No user-visible change, so no change file

## Verification

<!-- Commands run and what they showed. -->

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
