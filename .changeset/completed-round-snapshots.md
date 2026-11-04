---
default: patch
---

# Keep each route's statistics and address metadata on the same round

Reports use one completed-round snapshot for hop statistics, per-address round trips and MPLS label stacks, including when a trace ends because its route is stable or its time limit is reached.
