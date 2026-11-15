#!/usr/bin/env node
// Ersatz-Lighthouse für die unabhängigen Tests zu INTEG-130/132/143 (Quelle Lokal).
// Kein Netzwerk, kein Chrome: protokolliert jeden Aufruf in `calls.log` neben dieser Datei und
// - gibt bei `--version` die Version aus,
// - schreibt sonst einen minimalen Lighthouse-Bericht nach `--output-path=<Datei>`.
//   Enthält die URL „errored“, trägt der Bericht runtimeError ERRORED_DOCUMENT_REQUEST.
"use strict";
const fs = require("fs");
const path = require("path");

const VERSION = "12.4.1";
const args = process.argv.slice(2);
fs.appendFileSync(path.join(__dirname, "calls.log"), JSON.stringify(args) + "\n");

if (args.includes("--version")) {
  process.stdout.write(VERSION + "\n");
  process.exit(0);
}

const outArg = args.find((a) => a.startsWith("--output-path="));
const url = args.find((a) => /^https?:\/\//.test(a)) || "";
if (!outArg) process.exit(3);
const out = outArg.slice("--output-path=".length);

const report = {
  lighthouseVersion: VERSION,
  requestedUrl: url,
  finalUrl: url,
  fetchTime: "2026-11-15T12:00:00.000Z",
  categories: { performance: { id: "performance", score: 0.97 } },
  audits: {},
};
if (url.includes("errored")) {
  report.runtimeError = {
    code: "ERRORED_DOCUMENT_REQUEST",
    message: "The page could not be loaded (status code 401).",
  };
  delete report.categories;
}
fs.writeFileSync(out, JSON.stringify(report));
