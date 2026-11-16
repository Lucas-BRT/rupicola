"use strict";
const assert = require("assert");
const fs = require("fs");
const path = require("path");
const { project } = require("../scripts/rust-view.js");
const { scanSource } = require("../scripts/detect.js");

const text = fs.readFileSync(path.join(__dirname, "fixtures/page.rs"), "utf8");
const p = project(text);
assert.strictEqual(p.length, text.length, "offsets are kept");
assert.strictEqual(p.split("\n").length, text.split("\n").length, "lines are kept");
assert(!p.includes("violet"), "comments are blanked");
assert(!p.includes("not markup"), "code outside macros is blanked");
assert(p.includes("transition-all") && p.includes("tracking-[0.12em]"), "macro bodies are kept");

const rules = scanSource(text, "page.rs", { kind: "web" }).map((f) => `${f.rule}:${f.line}`);
for (const hit of ["transition-all:10", "gradient-text:11", "tracked-caps-label:4", "status-dot:12"]) {
  assert(rules.includes(hit), `${hit} in ${rules}`);
}
assert(!rules.some((r) => r.startsWith("slop-gradient")), "a commented gradient is not a finding");
console.log("ok");
