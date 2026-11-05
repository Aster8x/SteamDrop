// Keeps SteamDrop's version in sync across package.json, tauri.conf.json and Cargo.toml (and their lock files),
// and reads release notes from CHANGELOG.md.
//
//   node scripts/version.mjs bump 0.2.0   set the version everywhere and turn "Unreleased" into a 0.2.0 section
//   node scripts/version.mjs check [v0.2.0]  fail unless all files agree (and match the tag, and have a changelog section)
//   node scripts/version.mjs notes 0.2.0  print that version's changelog section
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const read = (f) => readFileSync(root + f, "utf8");
const write = (f, s) => writeFileSync(root + f, s);
const REPO = "https://github.com/Aster8x/SteamDrop";
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;
const fail = (msg) => { console.error(`error: ${msg}`); process.exit(1); };

// Each file's version, found with a pattern that matches only the app's own entry.
const files = [
  ["package.json", /("version": ")([^"]+)(")/],
  ["package-lock.json", /(^\{\r?\n  "name": "steamdrop",\r?\n  "version": ")([^"]+)(")/],
  ["package-lock.json", /("": \{\r?\n      "name": "steamdrop",\r?\n      "version": ")([^"]+)(")/],
  ["src-tauri/tauri.conf.json", /("version": ")([^"]+)(")/],
  ["src-tauri/Cargo.toml", /(\[package\][^[]*?\r?\nversion = ")([^"]+)(")/],
  ["src-tauri/Cargo.lock", /(name = "steamdrop"\r?\nversion = ")([^"]+)(")/],
];

function versions() {
  return files.map(([f, re]) => {
    const m = read(f).match(re);
    if (!m) fail(`no version found in ${f}`);
    return [f, m[2]];
  });
}

function notes(version) {
  const text = read("CHANGELOG.md").replace(/\r\n/g, "\n");
  const start = text.search(new RegExp(`^## \\[${version.replace(/\./g, "\\.")}\\]`, "m"));
  if (start < 0) return null;
  const body = text.slice(start).split("\n").slice(1);
  const end = body.findIndex((l) => /^## \[/.test(l) || /^\[[^\]]+\]: /.test(l));
  return body.slice(0, end < 0 ? undefined : end).join("\n").trim();
}

const [cmd, arg] = process.argv.slice(2);

if (cmd === "bump") {
  if (!SEMVER.test(arg || "")) fail("give a version like 0.2.0");
  const [[, current]] = versions();
  for (const [f, re] of files) write(f, read(f).replace(re, `$1${arg}$3`));
  let log = read("CHANGELOG.md");
  if (!notes(arg)) {
    const date = new Date().toISOString().slice(0, 10);
    log = log.replace(/^## \[Unreleased\]\r?\n/m, `## [Unreleased]\n\n## [${arg}] - ${date}\n`);
    log = log.replace(/^\[Unreleased\]: [^\r\n]*/m,
      `[Unreleased]: ${REPO}/compare/v${arg}...HEAD\n[${arg}]: ${REPO}/compare/v${current}...v${arg}`);
    write("CHANGELOG.md", log);
  }
  if (!notes(arg)) console.warn(`warning: the ${arg} section in CHANGELOG.md is empty; describe the changes before tagging.`);
  console.log(`Version is now ${arg} (was ${current}).`);
} else if (cmd === "check") {
  const list = versions();
  const want = arg ? arg.replace(/^v/, "") : list[0][1];
  const wrong = list.filter(([, v]) => v !== want);
  if (wrong.length) fail(`versions disagree (expected ${want}): ${list.map(([f, v]) => `${f}=${v}`).join(", ")}`);
  if (!SEMVER.test(want)) fail(`${want} is not a semantic version`);
  if (arg && !notes(want)) fail(`CHANGELOG.md has no notes under "## [${want}]"`);
  console.log(`All files say ${want}.`);
} else if (cmd === "notes") {
  const n = notes(arg || "");
  if (!n) fail(`CHANGELOG.md has no notes for ${arg}`);
  console.log(n);
} else {
  fail("usage: node scripts/version.mjs bump <version> | check [tag] | notes <version>");
}
