# The xmtr image: the static musl binary and nothing else. Build it with buildx from a context
# that holds <platform>/xmtr for every target platform, for example linux/amd64/xmtr.
# Docker grants CAP_NET_RAW by default, so the container can trace as its root user.
FROM scratch
ARG TARGETPLATFORM
COPY --chmod=0755 ${TARGETPLATFORM}/xmtr /usr/bin/xmtr
ENTRYPOINT ["/usr/bin/xmtr"]
