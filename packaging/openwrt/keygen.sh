#!/bin/sh
set -eu
umask 077

fail() {
	printf 'OpenWrt keygen: %s\n' "$*" >&2
	exit 1
}

[ "$#" -eq 1 ] || fail 'expected key format: ipk or apk'
: "${XMTR_KEY_DIR:?set XMTR_KEY_DIR to an absolute key directory}"
case "$XMTR_KEY_DIR" in
/*) ;;
*) fail "expected an absolute key directory: $XMTR_KEY_DIR" ;;
esac

case "$1" in
ipk)
	secret="$XMTR_KEY_DIR/xmtr-usign.key"
	public="$XMTR_KEY_DIR/xmtr-usign.pub"
	tool=/builder/staging_dir/host/bin/usign
	;;
apk)
	secret="$XMTR_KEY_DIR/xmtr-apk.key"
	public="$XMTR_KEY_DIR/xmtr-apk.pem"
	tool=/builder/staging_dir/host/bin/openssl
	;;
*) fail "unsupported key format: $1" ;;
esac
[ -x "$tool" ] || fail "SDK key tool is missing: $tool"
[ ! -e "$secret" ] && [ ! -e "$public" ] || fail "key files already exist in $XMTR_KEY_DIR"

mkdir -p "$XMTR_KEY_DIR"
[ "$(realpath "$XMTR_KEY_DIR")" != / ] || fail 'the key directory must not be the filesystem root'
chmod 0700 "$XMTR_KEY_DIR"
case "$1" in
ipk)
	"$tool" -G -s "$secret" -p "$public" -c 'xmtr OpenWrt repository'
	;;
apk)
	OPENSSL_CONF=/builder/staging_dir/host/etc/ssl/openssl.cnf
	export OPENSSL_CONF
	"$tool" ecparam -name prime256v1 -genkey -noout -out "$secret"
	"$tool" pkey -in "$secret" -pubout -out "$public"
	;;
esac
chmod 0600 "$secret"
chmod 0644 "$public"
printf '%s\n' "$public"
