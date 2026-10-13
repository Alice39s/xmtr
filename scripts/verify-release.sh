#!/usr/bin/env bash
# Installs a published release through one or more channels and checks the installed xmtr.
# Usage: scripts/verify-release.sh <version> <channel>...
# Channels: install-sh npm pypi cargo-binstall brew apt dnf apk docker
# Registries and the GitHub Pages cache can lag behind the release workflow, so each channel is
# tried for up to ten minutes. Every channel runs; the script fails if any of them failed.
set -euo pipefail
version="$1"
shift
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
raw="https://raw.githubusercontent.com/Alice39s/xmtr/v${version}"
pages="https://alice39s.github.io/xmtr"

# Installs xmtr through one channel and prints what the installed `xmtr --version` prints.
# Installer output goes to stderr. A failed step leaves the version empty or wrong.
installed_version() {
  local dir="${work}/$1"
  rm -rf "$dir"
  mkdir -p "$dir"
  case "$1" in
    install-sh)
      curl -fsSL "${raw}/install.sh" | bash -s -- --dir "${dir}/bin" --version "v${version}" >&2 &&
        "${dir}/bin/xmtr" --version
      ;;
    npm)
      npm install --global --silent --prefix "$dir" "@xmtr/cli@${version}" >&2 &&
        "${dir}/bin/xmtr" --version
      ;;
    pypi)
      python3 -m venv "${dir}/venv" &&
        "${dir}/venv/bin/pip" install --quiet --no-cache-dir "xmtr==${version}" >&2 &&
        "${dir}/venv/bin/xmtr" --version
      ;;
    cargo-binstall)
      cargo binstall --no-confirm --disable-strategies compile --root "$dir" "xmtr@${version}" >&2 &&
        "${dir}/bin/xmtr" --version
      ;;
    brew)
      HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ENV_HINTS=1 brew install --quiet alice39s/tap/xmtr >&2 &&
        "$(brew --prefix)/bin/xmtr" --version
      ;;
    apt)
      docker run --rm debian:trixie bash -euc "
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq >&2 && apt-get install -y -qq curl ca-certificates >&2
        curl -fsSL -o /usr/share/keyrings/xmtr.gpg ${pages}/xmtr.gpg
        echo 'deb [signed-by=/usr/share/keyrings/xmtr.gpg] ${pages}/deb stable main' > /etc/apt/sources.list.d/xmtr.list
        apt-get update -qq >&2 && apt-get install -y -qq xmtr=${version}-1 >&2
        xmtr --version"
      ;;
    dnf)
      docker run --rm fedora:latest bash -euc "
        curl -fsSL -o /etc/yum.repos.d/xmtr.repo ${pages}/rpm/xmtr.repo
        dnf install -y -q xmtr-${version} >&2
        xmtr --version"
      ;;
    apk)
      docker run --rm alpine:latest sh -euc "
        wget -qO /etc/apk/keys/xmtr.rsa.pub ${pages}/apk/xmtr.rsa.pub
        echo ${pages}/apk >> /etc/apk/repositories
        apk add -q xmtr=${version}-r1 >&2
        xmtr --version"
      ;;
    docker)
      docker run --rm "ghcr.io/alice39s/xmtr:${version}" --version
      ;;
    *)
      echo "unknown channel: $1" >&2
      exit 2
      ;;
  esac
}

verify() {
  local channel="$1"
  local attempt reported
  for attempt in $(seq 1 20); do
    reported="$(installed_version "$channel")" || reported=""
    if [ "$reported" = "xmtr ${version}" ]; then
      echo "PASS ${channel}: ${reported}"
      return 0
    fi
    echo "${channel}: attempt ${attempt} printed '${reported}', expected 'xmtr ${version}'; retrying in 30s" >&2
    sleep 30
  done
  echo "FAIL ${channel}"
  return 1
}

failed=""
for channel in "$@"; do
  verify "$channel" || failed="${failed} ${channel}"
done
if [ -n "$failed" ]; then
  echo "failed channels:${failed}" >&2
  exit 1
fi
