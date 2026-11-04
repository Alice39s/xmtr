#!/bin/sh
set -eu

fail() {
	printf 'OpenWrt index: %s\n' "$*" >&2
	exit 1
}

[ "$#" -eq 1 ] || fail 'expected package format: ipk or apk'
case "$1" in
ipk | apk) format=$1 ;;
*) fail "unsupported package format: $1" ;;
esac

: "${XMTR_FEED_DIR:?set XMTR_FEED_DIR to the absolute package directory}"
: "${XMTR_SIGNING_KEY:?set XMTR_SIGNING_KEY to an absolute private key path}"
: "${XMTR_PUBLIC_KEY:?set XMTR_PUBLIC_KEY to an absolute public key path}"
: "${TMPDIR:?set TMPDIR to a writable build directory}"
for path in "$XMTR_FEED_DIR" "$XMTR_SIGNING_KEY" "$XMTR_PUBLIC_KEY" "$TMPDIR"; do
	case "$path" in
	/*) ;;
	*) fail "expected an absolute path: $path" ;;
	esac
done

[ -d "$XMTR_FEED_DIR" ] && [ -w "$XMTR_FEED_DIR" ] || fail "feed directory is not writable: $XMTR_FEED_DIR"
[ -f "$XMTR_SIGNING_KEY" ] && [ -r "$XMTR_SIGNING_KEY" ] || fail "cannot read the signing key: $XMTR_SIGNING_KEY"
[ -f "$XMTR_PUBLIC_KEY" ] && [ -r "$XMTR_PUBLIC_KEY" ] || fail "cannot read the public key: $XMTR_PUBLIC_KEY"
[ -d "$TMPDIR" ] && [ -w "$TMPDIR" ] || fail "build directory is not writable: $TMPDIR"

SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-0}
export SOURCE_DATE_EPOCH
cd "$XMTR_FEED_DIR"

case "$format" in
ipk)
	set -- ./*.ipk
	[ -f "$1" ] || fail "no ipk packages in $XMTR_FEED_DIR"
	index=/builder/scripts/ipkg-make-index.sh
	usign=/builder/staging_dir/host/bin/usign
	[ -x "$index" ] && [ -x "$usign" ] || fail 'SDK ipk index tools are missing'
	MKHASH=/builder/staging_dir/host/bin/mkhash
	export MKHASH
	/bin/bash -o pipefail "$index" . > Packages
	# OpenWrt pads these index lengths before usign to handle its SHA-512 edge case.
	length=$(wc -c < Packages)
	case "$(((64 + length) % 128))" in
	110 | 111) printf '\n\n' >> Packages ;;
	esac
	gzip -9nc Packages > Packages.gz
	"$usign" -S -m Packages -s "$XMTR_SIGNING_KEY" -x Packages.sig
	"$usign" -V -m Packages -p "$XMTR_PUBLIC_KEY" -x Packages.sig
	printf '%s\n' "$XMTR_FEED_DIR/Packages.gz"
	;;
apk)
	set -- ./*.apk
	[ -f "$1" ] || fail "no apk packages in $XMTR_FEED_DIR"
	apk=/builder/staging_dir/host/bin/apk
	[ -x "$apk" ] || fail "SDK apk tool is missing: $apk"
	trust=$(mktemp -d "$TMPDIR/xmtr-apk-keys.XXXXXXXX")
	install -m 0644 "$XMTR_PUBLIC_KEY" "$trust/xmtr.pem"
	for package in "$@"; do
		# Signing consumes an unsigned build output; verify the final package against its public key.
		"$apk" --allow-untrusted verify "$package"
		"$apk" --allow-untrusted adbsign --reset-signatures --sign-key "$XMTR_SIGNING_KEY" "$package"
		"$apk" --keys-dir "$trust" verify "$package"
	done
	"$apk" --keys-dir "$trust" mkndx --sign-key "$XMTR_SIGNING_KEY" --output packages.adb "$@"
	"$apk" --keys-dir "$trust" verify packages.adb
	printf '%s\n' "$XMTR_FEED_DIR/packages.adb"
	;;
esac
