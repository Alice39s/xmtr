#!/bin/sh
set -eu

fail() {
	printf 'OpenWrt package: %s\n' "$*" >&2
	exit 1
}

[ "$#" -eq 1 ] || fail 'expected package format: ipk or apk'
case "$1" in
ipk | apk) format=$1 ;;
*) fail "unsupported package format: $1" ;;
esac

: "${XMTR_VERSION:?set XMTR_VERSION to the upstream version}"
: "${XMTR_BINARY:?set XMTR_BINARY to an absolute binary path}"
: "${XMTR_PACKAGE_ARCH:?set XMTR_PACKAGE_ARCH to the OpenWrt package architecture}"
: "${XMTR_OUTPUT:?set XMTR_OUTPUT to an absolute output directory}"
: "${TMPDIR:?set TMPDIR to a writable build directory}"

for path in "$XMTR_BINARY" "$XMTR_OUTPUT" "$TMPDIR"; do
	case "$path" in
	/*) ;;
	*) fail "expected an absolute path: $path" ;;
	esac
done

case "$XMTR_VERSION" in
*[!0-9A-Za-z.+~_-]*) fail "invalid package version: $XMTR_VERSION" ;;
esac
case "$XMTR_PACKAGE_ARCH" in
*[!0-9a-z_+-]*) fail "invalid OpenWrt architecture: $XMTR_PACKAGE_ARCH" ;;
esac

[ -f "$XMTR_BINARY" ] && [ -x "$XMTR_BINARY" ] || fail "binary is not executable: $XMTR_BINARY"
[ -d "$TMPDIR" ] && [ -w "$TMPDIR" ] || fail "build directory is not writable: $TMPDIR"

binary_dir=$(dirname "$XMTR_BINARY")
for license in LICENSE-MIT LICENSE-APACHE; do
	[ -f "$binary_dir/$license" ] || fail "release archive is missing $binary_dir/$license"
done

case "$format" in
ipk)
	tool=/builder/scripts/ipkg-build
	package_name="xmtr_${XMTR_VERSION}-r1_${XMTR_PACKAGE_ARCH}.ipk"
	;;
apk)
	tool=/builder/staging_dir/host/bin/apk
	package_name="xmtr-${XMTR_VERSION}-r1.apk"
	;;
esac
[ -x "$tool" ] || fail "SDK package tool is missing: $tool"

mkdir -p "$XMTR_OUTPUT"
output="$XMTR_OUTPUT/$package_name"
[ ! -e "$output" ] || fail "package already exists: $output"

work_dir=$(mktemp -d "$TMPDIR/xmtr-package.XXXXXXXX")
root="$work_dir/root"
install -D -m 0755 "$XMTR_BINARY" "$root/usr/bin/xmtr"
for license in LICENSE-MIT LICENSE-APACHE; do
	install -D -m 0644 "$binary_dir/$license" "$root/usr/share/licenses/xmtr/$license"
done

# The caller supplies an ephemeral build filesystem; SDK tools own their temporary files.
# Package the release binary directly so a host-architecture strip tool never rewrites it.
SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-0}
export SOURCE_DATE_EPOCH
STAGING_DIR_HOST=/builder/staging_dir/host
export STAGING_DIR_HOST

case "$format" in
ipk)
	install -d -m 0755 "$root/CONTROL"
	printf '%s\n' \
		'Package: xmtr' \
		"Version: ${XMTR_VERSION}-r1" \
		"Architecture: $XMTR_PACKAGE_ARCH" \
		'Maintainer: Alice39s <xmtr.opensource@xmsl.dev>' \
		'License: MIT OR Apache-2.0' \
		'Section: net' \
		'URL: https://github.com/Alice39s/xmtr' \
		'Installed-Size: 0' \
		'Description: Concurrent traceroute that maps every hop on xmsl.dev IPMap' \
		> "$root/CONTROL/control"
	# ipkg-build uses TAR for each archive, including control and outer members.
	TAR='/bin/tar --owner=0 --group=0'
	export TAR
	/bin/bash -o pipefail "$tool" "$root" "$XMTR_OUTPUT"
	;;
apk)
	fakeroot=/builder/staging_dir/host/bin/fakeroot
	[ -x "$fakeroot" ] || fail "SDK fakeroot is missing: $fakeroot"
	"$fakeroot" -- "$tool" mkpkg \
		--info name:xmtr \
		--info "version:${XMTR_VERSION}-r1" \
		--info "arch:$XMTR_PACKAGE_ARCH" \
		--info 'license:MIT OR Apache-2.0' \
		--info 'maintainer:Alice39s <xmtr.opensource@xmsl.dev>' \
		--info 'description:Concurrent traceroute that maps every hop on xmsl.dev IPMap' \
		--info 'url:https://github.com/Alice39s/xmtr' \
		--files "$root" \
		--output "$output"
	;;
esac

[ -f "$output" ] || fail "SDK did not create $output"
printf '%s\n' "$output"
