# Agent instructions

## Change files are required

Release notes and version bumps come only from change files in `.changeset/`. knope ignores commit messages (`ignore_conventional_commits = true` in `knope.toml`), so a change without a change file is missing from `CHANGELOG.md` and from the GitHub release.

Every pull request that changes what users install or run adds one change file, in the same pull request. This covers the `xmtr` binary, the installer, `install.sh`, `install.ps1`, the published packages and the README instructions they follow. Changes to tests, CI or internal docs only need a change file when they change a published artifact.

Create the file with `knope document-change`, or write it by hand:

```markdown
---
default: patch
---

# Survive a closed stdout and print gateway text safely

What went wrong or was missing before, and what happens now. ([#19](https://github.com/Alice39s/xmtr/pull/19), fixes [#12](https://github.com/Alice39s/xmtr/issues/12))
```

- `default` is the bump: `patch` for fixes, `minor` for new features, `major` for breaking changes.
- The heading becomes the changelog entry title. Describe the result for users, not the code change.
- End the body with a link to the pull request and to the issue it fixes. Open the pull request first if the number is not known yet, then add the link in a follow-up commit on the same branch.
- One file per user-visible change. A pull request that fixes two unrelated things adds two files.
- Do not edit `CHANGELOG.md` by hand; `knope release` writes it and deletes the change files.

## Pull requests

Fill in `.github/pull_request_template.md`. Before asking for review, run with Rust 1.88.0 (the workspace MSRV):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```
