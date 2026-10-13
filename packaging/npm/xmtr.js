#!/usr/bin/env node
// Runs the xmtr binary from the @xmtr/<platform> package that npm installed with this one.
"use strict";

const { spawnSync } = require("node:child_process");
const path = require("node:path");

const platform = `${process.platform}-${process.arch}`;
const name = `@xmtr/${platform}`;
const { optionalDependencies } = require("../package.json");
if (!Object.hasOwn(optionalDependencies, name)) {
  console.error(
    `xmtr has no npm build for ${platform}. Other ways to install it: https://github.com/Alice39s/xmtr#install`,
  );
  process.exit(1);
}

// Throws when npm skipped the optional dependency, for example with --omit=optional.
const dir = path.dirname(require.resolve(`${name}/package.json`));
const binary = path.join(dir, process.platform === "win32" ? "xmtr.exe" : "xmtr");
const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  throw result.error;
}
if (result.signal) {
  process.kill(process.pid, result.signal);
}
process.exitCode = result.status;
