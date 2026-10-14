## 0.3.0 (2026-10-14)

### Features

#### Show progress and results in a terminal panel

In a terminal, xmtr now draws a panel below the prompt while it runs. Numbered steps (1. Resolve, 2. Trace, 3. Lookup, 4. Upload, 5. Result) show which step is active, each route has a progress bar, reverse DNS and AS lookups show how many addresses have answered, and the upload shows that it is still waiting for the gateway.

When the run ends, the panel turns into the result: the IPMap link first, then notes about skipped targets and failed routes, then each route as a table. Runs of three or more silent hops fold into one line, and long host names are shortened before their addresses.

Ctrl-C stops a run and exits with status 130. Piped or redirected output, `--quiet`, and terminals that do not report the cursor position keep the plain text output.
