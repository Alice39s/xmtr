#!/usr/bin/env bash
# Builds the signed apt, dnf and apk repositories for GitHub Pages from the packages that
# scripts/linux-packages.sh wrote.
# Usage: scripts/linux-repos.sh <packages-dir> <site-dir>
# Needs reprepro, createrepo_c, docker, a GPG secret key for GPG_KEY_ID in the keyring, and
# APK_SIGNING_KEY_FILE, the RSA key that signed the apk packages.
set -euo pipefail
packages="$(cd "$1" && pwd)"
site="$2"
root="$(cd "$(dirname "$0")/.." && pwd)"
: "${GPG_KEY_ID:?GPG_KEY_ID names the signing key}"
: "${APK_SIGNING_KEY_FILE:?APK_SIGNING_KEY_FILE is the apk signing key}"
base="https://alice39s.github.io/xmtr"

# The keys must match the public keys that users install from packaging/keys.
published="$(gpg --batch --with-colons --show-keys "$root/packaging/keys/xmtr.asc" | awk -F: '$1 == "fpr" { print $10; exit }')"
[[ "$GPG_KEY_ID" == "$published" ]] || { echo "GPG key $GPG_KEY_ID is not packaging/keys/xmtr.asc ($published)" >&2; exit 1; }
openssl rsa -in "$APK_SIGNING_KEY_FILE" -pubout 2>/dev/null | cmp -s - "$root/packaging/keys/xmtr.rsa.pub" ||
  { echo "APK_SIGNING_KEY_FILE does not match packaging/keys/xmtr.rsa.pub" >&2; exit 1; }

rm -rf "$site"
mkdir -p "$site/rpm" "$site/apk"
site="$(cd "$site" && pwd)"
cp "$root/packaging/keys/xmtr.asc" "$site/xmtr.asc"
gpg --batch --yes --dearmor --output "$site/xmtr.gpg" "$root/packaging/keys/xmtr.asc"
cp "$root/packaging/keys/xmtr.rsa.pub" "$site/apk/xmtr.rsa.pub"

# apt: reprepro keeps its database in a work directory next to the site and writes dists/ and pool/ to the site.
work="$(mktemp -d "$(dirname "$site")/reprepro.XXXXXX")"
mkdir -p "$work/conf"
architectures="$(for deb in "$packages"/*.deb; do dpkg-deb --field "$deb" Architecture; done | sort -u | tr '\n' ' ')"
cat > "$work/conf/distributions" <<EOF
Origin: xmtr
Label: xmtr
Codename: stable
Architectures: ${architectures}
Components: main
Description: xmtr packages
SignWith: ${GPG_KEY_ID}
EOF
reprepro --basedir "$work" --outdir "$site/deb" includedeb stable "$packages"/*.deb
rm -rf "$work"

# dnf: one repository for every architecture, with a signed repomd.xml.
cp "$packages"/*.rpm "$site/rpm/"
createrepo_c --quiet "$site/rpm"
gpg --batch --yes --local-user "$GPG_KEY_ID" --armor --detach-sign "$site/rpm/repodata/repomd.xml"
cat > "$site/rpm/xmtr.repo" <<EOF
[xmtr]
name=xmtr
baseurl=${base}/rpm
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=${base}/xmtr.asc
EOF

# apk: one directory per architecture, each with an APKINDEX signed by the same key as the packages.
# nFPM names apk packages xmtr_<version>-r<release>_<arch>.apk; apk fetches xmtr-<version>-r<release>.apk.
for apk in "$packages"/*.apk; do
  file="${apk##*/}"
  arch="${file#*-r*_}"
  arch="${arch%.apk}"
  release="${file%_"$arch".apk}"
  mkdir -p "$site/apk/$arch"
  cp "$apk" "$site/apk/$arch/xmtr-${release#xmtr_}.apk"
done
docker run --rm -v "$site/apk:/repo" -v "$(cd "$(dirname "$APK_SIGNING_KEY_FILE")" && pwd)/$(basename "$APK_SIGNING_KEY_FILE"):/keys/xmtr.rsa:ro" \
  alpine:3.22 sh -euc '
    apk add --quiet abuild
    cp /repo/xmtr.rsa.pub /etc/apk/keys/xmtr.rsa.pub
    for dir in /repo/*/; do
      cd "$dir"
      apk index --quiet --output APKINDEX.tar.gz ./*.apk
      abuild-sign -q -k /keys/xmtr.rsa -p xmtr.rsa.pub APKINDEX.tar.gz
    done
    chown -R "$(stat -c %u:%g /repo)" /repo
  '
find "$site" -type f | sort
