---
default: minor
---

# Ask before sending error and crash reports

After a run with errors, or a crash, xmtr asks whether to send a report to the xmtr developers; the default answer is no, and nothing is sent without a terminal to ask. IP addresses, the hosts you named, your home directory and user names in paths are removed first. One run in 100 that uploads also sends the xmtr version, the OS with its distribution and version, and the CPU architecture. `DO_NOT_TRACK=1` turns off every report and question. See the Telemetry section of the README.
