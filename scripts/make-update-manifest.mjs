// Builds the `latest.json` that the in-app updater reads.
//
// Run it after `npm run tauri build` (with TAURI_SIGNING_PRIVATE_KEY set, so the
// bundle step emits the .sig file), then upload BOTH the installer and
// latest.json as assets on the GitHub release.
//
//   node scripts/make-update-manifest.mjs [--notes "what changed"]

import { readFileSync, writeFileSync, readdirSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const conf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
const version = conf.version;
const repo = "fdeox/spotiamp-plus";

const bundleDir = join(root, "src-tauri/target/release/bundle/nsis");
if (!existsSync(bundleDir)) {
  console.error(`No bundle directory at ${bundleDir} — run \`npm run tauri build\` first.`);
  process.exit(1);
}

const sigName = readdirSync(bundleDir).find((f) => f.includes(version) && f.endsWith(".exe.sig"));
if (!sigName) {
  console.error(
    `No .sig for ${version} in ${bundleDir}.\n` +
      "The build must run with TAURI_SIGNING_PRIVATE_KEY set, otherwise it can't sign the update.",
  );
  process.exit(1);
}
const installerName = sigName.replace(/\.sig$/, "");
const signature = readFileSync(join(bundleDir, sigName), "utf8").trim();

const notesFlag = process.argv.indexOf("--notes");
const notes = notesFlag !== -1 ? process.argv[notesFlag + 1] : `Spotiamp+ ${version}`;

// The tag is created by the GitHub release; keep it in the v-prefixed form the
// project has used since v0.6.4.
const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature,
      url: `https://github.com/${repo}/releases/download/v${version}/${encodeURIComponent(installerName)}`,
    },
  },
};

const outDir = join(root, "releases");
mkdirSync(outDir, { recursive: true });
const out = join(outDir, "latest.json");
writeFileSync(out, JSON.stringify(manifest, null, 2));

console.log(`Wrote ${out}`);
console.log(`  version   ${version}`);
console.log(`  installer ${installerName}`);
console.log(`  url       ${manifest.platforms["windows-x86_64"].url}`);
console.log("\nUpload BOTH the installer and latest.json to the GitHub release.");
