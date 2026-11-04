---
default: patch
---

# Upload the best round trip of each address at a hop

When several addresses answer at the same TTL, the round-trip statistics of the hop mix all of their replies. The upload now also carries the lowest round trip of each address's own replies, so IPMap can bound how far each address is from the source by its own replies. ([#36](https://github.com/Alice39s/xmtr/pull/36))
