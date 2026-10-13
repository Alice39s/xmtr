#!/usr/bin/env bash
# Starts the xmtr and xmtr-installer binaries that scripts/package.sh packed for one target.
# Usage: scripts/smoke.sh <target> <dist-dir> [runner...]
# The runner, such as `qemu-riscv64 -L /usr/riscv64-linux-gnu`, starts binaries built for another CPU.
# With XMTR_SMOKE_TRACE=1 xmtr also traces 127.0.0.1, which needs raw sockets.
set -euo pipefail
target="$1"
dist="$2"
shift 2
version="$(awk -F'"' '/^\[workspace\.package\]/ { section = 1 } section && /^version = / { print $2; exit }' Cargo.toml)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

case "$target" in
  *windows*)
    unzip -q "${dist}/xmtr-${target}.zip" -d "$work"
    xmtr="${work}/xmtr.exe"
    cp "${dist}/xmtr-installer-${target}.exe" "${work}/xmtr-installer.exe"
    installer="${work}/xmtr-installer.exe"
    ;;
  *)
    tar -xzf "${dist}/xmtr-${target}.tar.gz" -C "$work"
    xmtr="${work}/xmtr"
    # Artifacts do not keep file modes, so the bare installer needs its execute bit back.
    cp "${dist}/xmtr-installer-${target}" "${work}/xmtr-installer"
    chmod +x "${work}/xmtr-installer"
    installer="${work}/xmtr-installer"
    ;;
esac

fail() {
  echo "${target}: $*" >&2
  exit 1
}

reported="$("$@" "$xmtr" --version)"
[ "$reported" = "xmtr ${version}" ] || fail "--version printed '${reported}', expected 'xmtr ${version}'"

# Output is captured before grep reads it: `grep -q` would close the pipe early, and Rust's
# println! aborts on a closed stdout.
help="$("$@" "$xmtr")" || fail "xmtr without arguments exited with $?"
grep -q '^Usage: xmtr' <<<"$help" || fail "xmtr without arguments printed no usage"

status=0
"$@" "$xmtr" --no-upload >/dev/null 2>&1 || status=$?
[ "$status" = 2 ] || fail "options without a target exited with ${status}, expected 2"

installer_help="$("$@" "$installer" --help)" || fail "xmtr-installer --help exited with $?"
grep -q '^Usage: xmtr-installer' <<<"$installer_help" || fail "xmtr-installer --help printed no usage"

if [ "${XMTR_SMOKE_TRACE:-}" = 1 ]; then
  sudo=()
  case "${OSTYPE:-}" in
    msys* | cygwin*) ;;
    *) [ "$(id -u)" = 0 ] || sudo=(sudo) ;;
  esac
  ${sudo[@]+"${sudo[@]}"} "$@" "$xmtr" --no-upload --no-rdns --max-rounds 2 --min-rounds 1 127.0.0.1 >"${work}/trace.txt" 2>&1 ||
    fail "tracing 127.0.0.1 failed: $(cat "${work}/trace.txt")"
  cat "${work}/trace.txt"
  grep -q ', reached' "${work}/trace.txt" || fail "the trace to 127.0.0.1 did not reach the target"
fi

echo "${target}: ok"
