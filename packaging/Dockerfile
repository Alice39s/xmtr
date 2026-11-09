# The xmtr image: the static musl binary and root's entry in /etc/passwd. Build it with buildx
# from a context that holds <platform>/xmtr for every target platform, for example linux/amd64/xmtr.
# Docker grants CAP_NET_RAW by default, so the container can trace as its root user.
FROM scratch
ARG TARGETPLATFORM
COPY --chmod=0755 ${TARGETPLATFORM}/xmtr /usr/bin/xmtr
# root's entry in the user database: xmtr keeps the trace list in root's home directory, here in the
# container's writable layer.
COPY --chmod=0644 <<EOF /etc/passwd
root:x:0:0:root:/root:/sbin/nologin
EOF
ENTRYPOINT ["/usr/bin/xmtr"]
