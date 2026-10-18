#!/usr/bin/env node
// Keeps the app version identical in every file that carries it.
//
//   node scripts/version.mjs patch|minor|major   bump and print the new version
//   node scripts/version.mjs 1.2.3               set an exact version
//   node scripts/version.mjs check [1.2.3]       fail if the files disagree
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (f) => readFileSync(join(root, f), "utf8");
const write = (f, s) => writeFileSync(join(root, f), s);
const SEMVER = /^\d+\.\d+\.\d+$/;

const json = (file, update) => ({
  get: () => JSON.parse(read(file)).version,
  set: (v) => {
    const data = JSON.parse(read(file));
    update(data, v);
    write(file, JSON.stringify(data, null, 2) + "\n");
  },
});

const regex = (file, re) => ({
  get: () => re.exec(read(file))?.[2],
  set: (v) => write(file, read(file).replace(re, (_, head, _old, tail) => `${head}${v}${tail}`)),
});

const files = {
  "package.json": json("package.json", (d, v) => (d.version = v)),
  "package-lock.json": json("package-lock.json", (d, v) => {
    d.version = v;
    if (d.packages?.[""]) d.packages[""].version = v;
  }),
  "src-tauri/tauri.conf.json": json("src-tauri/tauri.conf.json", (d, v) => (d.version = v)),
  "src-tauri/Cargo.toml": regex("src-tauri/Cargo.toml", /(\[package\][^[]*?\nversion = ")([^"]+)(")/),
  "src-tauri/Cargo.lock": regex("src-tauri/Cargo.lock", /(name = "blendstack"\nversion = ")([^"]+)(")/),
};

const current = files["package.json"].get();
const arg = process.argv[2];

if (arg === "check") {
  const expected = process.argv[3] ?? current;
  const wrong = Object.entries(files).filter(([, f]) => f.get() !== expected);
  for (const [name, f] of wrong) console.error(`${name}: ${f.get()} (expected ${expected})`);
  if (wrong.length) process.exit(1);
  console.log(expected);
} else if (arg) {
  let next = arg;
  if (!SEMVER.test(arg)) {
    const [major, minor, patch] = current.split(".").map(Number);
    next =
      arg === "major" ? `${major + 1}.0.0` : arg === "minor" ? `${major}.${minor + 1}.0` : arg === "patch" ? `${major}.${minor}.${patch + 1}` : "";
  }
  if (!SEMVER.test(next)) {
    console.error(`unknown version or bump: ${arg}`);
    process.exit(1);
  }
  for (const f of Object.values(files)) f.set(next);
  console.log(next);
} else {
  console.log(current);
}
