// Writes src-tauri/THIRD-PARTY-NOTICES.txt: every open source package that
// ships inside Spotiamp+ (the Rust crates compiled into the app and the web
// packages bundled into its pages), each with its license, then the license
// texts themselves (identical texts once). The installer puts it in the
// licenses folder, next to projectM's and GLEW's.
//
// Runs before every build (beforeBuildCommand). It reads the local cargo cache
// and node_modules only; if anything fails, the existing file is kept.
//
//   node scripts/third-party-notices.mjs

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, readdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "src-tauri", "THIRD-PARTY-NOTICES.txt");
const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|unlicense)([-._].*)?$/i;

const licenseFiles = (dir) =>
  existsSync(dir)
    ? readdirSync(dir, { withFileTypes: true })
        .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
        .map((e) => e.name)
        .sort()
    : [];

// The same text with different line endings or trailing spaces is one text.
const norm = (text) => text.replace(/\r\n?/g, "\n").replace(/[ \t]+\n/g, "\n").trim();

function rustCrates() {
  const meta = JSON.parse(
    execFileSync(
      "cargo",
      ["metadata", "--format-version", "1", "--offline", "--filter-platform", "x86_64-pc-windows-msvc"],
      { cwd: join(root, "src-tauri"), maxBuffer: 256 * 1024 * 1024, encoding: "utf8" },
    ),
  );
  const pkgs = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const app = meta.resolve.root;
  // what ends up in the program: normal dependencies, not build or dev ones
  const seen = new Set();
  const stack = [app];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const dep of nodes.get(id).deps) {
      if (dep.dep_kinds.some((k) => k.kind === null)) stack.push(dep.pkg);
    }
  }
  seen.delete(app);
  return [...seen].map((id) => {
    const p = pkgs.get(id);
    const dir = dirname(p.manifest_path);
    return {
      name: p.name,
      version: p.version,
      license: p.license || (p.license_file ? `see ${p.license_file}` : "unknown"),
      source: p.repository || "",
      dir,
      files: p.license_file ? [p.license_file] : licenseFiles(dir),
    };
  });
}

// The web packages whose code is bundled into the pages: the app's own
// dependencies, plus Svelte and SvelteKit, whose runtime the pages carry.
function webPackages() {
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const names = [...Object.keys(pkg.dependencies || {}), "svelte", "@sveltejs/kit"];
  return names.map((name) => {
    const dir = join(root, "node_modules", ...name.split("/"));
    const meta = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    const repo = typeof meta.repository === "string" ? meta.repository : meta.repository?.url || "";
    return { name, version: meta.version, license: meta.license || "unknown", source: repo, dir, files: licenseFiles(dir) };
  });
}

function build() {
  const rust = rustCrates().sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
  const web = webPackages().sort((a, b) => a.name.localeCompare(b.name));

  // license texts, each once, with the packages that ship it
  const texts = new Map();
  const noText = [];
  for (const p of [...web, ...rust]) {
    if (!p.files.length) noText.push(p);
    for (const f of p.files) {
      const text = norm(readFileSync(join(p.dir, f), "utf8"));
      if (!texts.has(text)) texts.set(text, []);
      texts.get(text).push(`${p.name} ${p.version}`);
    }
  }

  const row = (p) => `  ${`${p.name} ${p.version}`.padEnd(44)} ${p.license}`;
  const lines = [
    "Spotiamp+ third-party notices",
    "=============================",
    "",
    "Spotiamp+ is free software under the MIT License (see LICENSE in its",
    "repository). It is built with the open source software listed below, each",
    "under its own license; their license texts follow the lists.",
    "",
    "Also in this folder: the licenses of projectM (MilkDrop) and GLEW. The",
    "MilkDrop presets and their authors are listed in presets/CREDITS.txt.",
    "",
    "Also included:",
    "  - The Winamp skins that come with Spotiamp+: the Classified series by",
    "    Victhor, and the Sony and Nucleo skins. All rights remain with their",
    "    original authors.",
    "  - px sans nouveaux, the pixel font, carried over from the original",
    "    Spotiamp by Ted Steen (https://github.com/tedsteen/Spotiamp).",
    "",
    `Web interface (${web.length})`,
    "-".repeat(`Web interface (${web.length})`.length),
    ...web.map(row),
    "",
    `Rust crates (${rust.length})`,
    "-".repeat(`Rust crates (${rust.length})`.length),
    ...rust.map(row),
    "",
  ];
  if (noText.length) {
    lines.push(
      "These ship no license file of their own; they are under the license named",
      "above, whose standard text is among the texts below:",
      ...noText.map((p) => `  ${p.name} ${p.version} (${p.license})`),
      "",
    );
  }
  lines.push("License texts", "=============", "");
  for (const [text, users] of texts) {
    lines.push(`---- ${users.join(", ")} ----`.slice(0, 2000), "", text, "", "");
  }
  return lines.join("\n");
}

try {
  writeFileSync(out, build());
  console.log(`third-party notices: ${out}`);
} catch (e) {
  console.warn(`third-party notices not updated (${e.message.split("\n")[0]}); keeping the existing file`);
}
