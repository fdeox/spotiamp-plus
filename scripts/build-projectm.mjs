// Builds libprojectM (the MilkDrop engine behind the MilkDrop visualizer) and
// fetches GLEW, into src-tauri/milkdrop/: projectM-4.dll, glew32.dll and their
// licences. The installer ships them next to spotiamp.exe.
//
// Both downloads are pinned by SHA-256. Needs CMake (or CMAKE=<path to
// cmake.exe>) and the MSVC build tools Rust already uses. Runs before every
// build (tauri.conf.json) and does nothing once the DLLs are there.
//
//   node scripts/build-projectm.mjs [--force]
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const OUT = join(ROOT, "src-tauri", "milkdrop");
const PROJECTM = {
  version: "4.1.7",
  url: "https://github.com/projectM-visualizer/projectm/releases/download/v4.1.7/libprojectM-4.1.7.tar.gz",
  sha256: "16e4610b4d58981f7a2351faa58d507d3f40163a7b26ba81e19aec928c66641f",
};
// projectM 4.1 reaches OpenGL through GLEW on Windows
const GLEW = {
  version: "2.2.0",
  url: "https://github.com/nigels-com/glew/releases/download/glew-2.2.0/glew-2.2.0-win32.zip",
  sha256: "ea6b14a1c6c968d0034e61ff6cb242cff2ce0ede79267a0f2b47b1b0b652c164",
};
const STAMP = join(OUT, "VERSIONS.txt");
const STAMP_TEXT = `projectM ${PROJECTM.version} (static C runtime)\n  source: ${PROJECTM.url}\nGLEW ${GLEW.version}\n  source: ${GLEW.url}\n`;
const OUTPUTS = ["projectM-4.dll", "glew32.dll", "LICENSE-projectM.txt", "LICENSE-GLEW.txt", "VERSIONS.txt"];

if (process.platform !== "win32") {
  console.log("build-projectm: Windows only, skipped");
  process.exit(0);
}
const force = process.argv.includes("--force");
if (!force && OUTPUTS.every((f) => existsSync(join(OUT, f))) && readFileSync(STAMP, "utf8") === STAMP_TEXT) {
  process.exit(0); // built already
}

// MSBuild's file tracking breaks on long paths: build somewhere short
const WORK = join(tmpdir(), "spotiamp-projectm");
// Windows' own tar (bsdtar) opens .tar.gz and .zip alike
const TAR = join(process.env.SystemRoot || "C:\\Windows", "System32", "tar.exe");
const CMAKE = process.env.CMAKE || "cmake";

/** @param {string} cmd @param {string[]} args @param {string} [cwd] */
function run(cmd, args, cwd = WORK) {
  execFileSync(cmd, args, { cwd, stdio: ["ignore", "inherit", "inherit"] });
}

/** @param {{url: string, sha256: string}} dep @param {string} file */
async function fetchPinned(dep, file) {
  const path = join(WORK, file);
  const hash = (/** @type {Buffer} */ b) => createHash("sha256").update(b).digest("hex");
  if (existsSync(path) && hash(readFileSync(path)) === dep.sha256) return path;
  const res = await fetch(dep.url);
  if (!res.ok) throw new Error(`download failed (${res.status}): ${dep.url}`);
  const data = Buffer.from(await res.arrayBuffer());
  if (hash(data) !== dep.sha256) throw new Error(`checksum mismatch: ${dep.url}`);
  writeFileSync(path, data);
  return path;
}

try {
  execFileSync(CMAKE, ["--version"], { stdio: "ignore" });
} catch {
  console.error("build-projectm: CMake not found. Install it, or set CMAKE to the full path of cmake.exe.");
  process.exit(1);
}

console.log(`build-projectm: building projectM ${PROJECTM.version} (once; a few minutes)`);
mkdirSync(WORK, { recursive: true });
const tarball = await fetchPinned(PROJECTM, "libprojectM.tar.gz");
const glewZip = await fetchPinned(GLEW, "glew.zip");
const src = join(WORK, `libprojectM-${PROJECTM.version}`);
const glew = join(WORK, `glew-${GLEW.version}`);
for (const dir of [src, glew, join(WORK, "b")]) rmSync(dir, { recursive: true, force: true });
run(TAR, ["-xf", tarball]);
run(TAR, ["-xf", glewZip]);

run(CMAKE, [
  "-S", src, "-B", "b", "-A", "x64",
  "-DBUILD_SHARED_LIBS=ON",
  // the C/C++ runtime inside the DLL: no MSVCP140.dll needed, which a fresh
  // Windows without the Visual C++ Redistributable doesn't have
  "-DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded",
  "-DENABLE_PLAYLIST=OFF",
  // the copies vendored in the tarball, not system packages
  "-DENABLE_SYSTEM_PROJECTM_EVAL=OFF",
  "-DENABLE_SYSTEM_GLM=OFF",
  `-DGLEW_ROOT=${glew}`,
  `-DCMAKE_PREFIX_PATH=${glew}`,
]);
run(CMAKE, ["--build", "b", "--config", "Release", "--parallel"]);

mkdirSync(OUT, { recursive: true });
copyFileSync(join(WORK, "b", "src", "libprojectM", "Release", "projectM-4.dll"), join(OUT, "projectM-4.dll"));
copyFileSync(join(glew, "bin", "Release", "x64", "glew32.dll"), join(OUT, "glew32.dll"));
copyFileSync(join(src, "LICENSE.txt"), join(OUT, "LICENSE-projectM.txt"));
copyFileSync(join(glew, "LICENSE.txt"), join(OUT, "LICENSE-GLEW.txt"));
writeFileSync(STAMP, STAMP_TEXT);
console.log(`build-projectm: done, ${OUT}`);
