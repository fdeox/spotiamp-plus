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
// the additions' code is in the stamp too (its hash), so a change to it builds again
const ADDITIONS = createHash("sha256")
  .update(readFileSync(join(ROOT, "scripts", "projectm", "SpotiampFft.hpp"), "utf8").replaceAll("
", "
")) // same on any checkout
  .digest("hex")
  .slice(0, 12);
const STAMP_TEXT = `projectM ${PROJECTM.version} (static C runtime) + Spotiamp+'s get_fft / get_wave (scripts/projectm, ${ADDITIONS})\n  source: ${PROJECTM.url}\nGLEW ${GLEW.version}\n  source: ${GLEW.url}\n`;
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

// Spotiamp+'s additions to projectM, made to the fresh source: get_fft() and
// get_fft_peak() for presets made for BeatDrop / MilkDrop 3 (the code is in
// scripts/projectm/SpotiampFft.hpp). Each edit has to find its spot exactly
// once, so a projectM update that moves things stops the build instead of
// quietly leaving one out.
/** @param {string} lib src/libprojectM of the source */
function patchProjectM(lib) {
  copyFileSync(join(ROOT, "scripts", "projectm", "SpotiampFft.hpp"), join(lib, "MilkdropPreset", "SpotiampFft.hpp"));
  /** @param {string} file @param {[string, string][]} edits */
  const edit = (file, edits) => {
    const path = join(lib, file);
    let text = readFileSync(path, "utf8");
    const eol = text.includes("\r\n") ? "\r\n" : "\n";
    for (const [from, to] of edits) {
      const [a, b] = [from, to].map((s) => s.replaceAll("\n", eol));
      const parts = text.split(a);
      if (parts.length !== 2) throw new Error(`build-projectm: the spot to patch in ${file} isn't there once: ${from.slice(0, 70)}`);
      text = parts.join(b);
    }
    writeFileSync(path, text);
  };
  edit("Renderer/TextureManager.hpp", [
    [
      "    auto GetSampler(const std::string& fullName) -> std::shared_ptr<class Sampler>;\n",
      "    auto GetSampler(const std::string& fullName) -> std::shared_ptr<class Sampler>;\n\n" +
        "    //! Spotiamp+: get_fft()'s spectrum texture (SpotiampFft.hpp), one per projectM instance.\n" +
        "    std::shared_ptr<class Texture> spotiampFft;\n" +
        "    //! Spotiamp+: get_wave()'s waveform texture (SpotiampFft.hpp), one per projectM instance.\n" +
        "    std::shared_ptr<class Texture> spotiampWave;\n",
    ],
  ]);
  edit("MilkdropPreset/PresetState.hpp", [
    [
      "    libprojectM::Audio::FrameAudioData audioData; //!< Holds audio/spectrum data and values for beat detection.\n",
      "    libprojectM::Audio::FrameAudioData audioData; //!< Holds audio/spectrum data and values for beat detection.\n" +
        "    float fftAttack{0.5f}; //!< Spotiamp+: BeatDrop's FFTAttack, how quickly get_fft() rises.\n" +
        "    float fftDecay{0.7f};  //!< Spotiamp+: BeatDrop's FFTDecay, how slowly it falls.\n",
    ],
  ]);
  edit("MilkdropPreset/PresetState.cpp", [
    [
      '    decay = parsedFile.GetFloat("fDecay", decay);\n',
      '    decay = parsedFile.GetFloat("fDecay", decay);\n' +
        '    fftAttack = parsedFile.GetFloat("FFTAttack", fftAttack);\n' +
        '    fftDecay = parsedFile.GetFloat("FFTDecay", fftDecay);\n',
    ],
  ]);
  edit("MilkdropPreset/MilkdropShader.cpp", [
    ['#include "Utils.hpp"\n', '#include "Utils.hpp"\n#include "SpotiampFft.hpp"\n'],
    [
      "    m_fragmentShaderCode = presetShaderCode;\n    m_preprocessedCode = m_fragmentShaderCode;\n",
      "    m_fragmentShaderCode = presetShaderCode;\n" +
        "    // Spotiamp+: BeatDrop's get_fft() and friends, for the presets that call them\n" +
        "    m_fragmentShaderCode = SpotiampFft::Prepare(m_fragmentShaderCode);\n" +
        "    m_preprocessedCode = m_fragmentShaderCode;\n",
    ],
    [
      "        // Random textures need special treatment.\n",
      "        // Spotiamp+: get_fft()'s spectrum texture\n" +
        '        if (lowerCaseName == "fft")\n' +
        "        {\n" +
        "            m_textureSamplerDescriptors.emplace_back(SpotiampFft::Get().FftTexture(*presetState.renderContext.textureManager),\n" +
        '                                                      presetState.renderContext.textureManager->GetSampler("fc_fft"),\n' +
        '                                                      name, "fft");\n' +
        "            continue;\n" +
        "        }\n" +
        "        // Spotiamp+: get_wave()'s waveform texture\n" +
        '        if (lowerCaseName == "wave")\n' +
        "        {\n" +
        "            m_textureSamplerDescriptors.emplace_back(SpotiampFft::Get().WaveTexture(*presetState.renderContext.textureManager),\n" +
        '                                                      presetState.renderContext.textureManager->GetSampler("fc_wave"),\n' +
        '                                                      name, "wave");\n' +
        "            continue;\n" +
        "        }\n\n" +
        "        // Random textures need special treatment.\n",
    ],
    [
      "    m_shader.Bind();\n\n    m_shader.SetUniformMat4x4(\"vertex_transformation\"",
      "    // Spotiamp+: get_fft()'s spectrum, once a frame\n" +
        "    SpotiampFft::Get().Update(*presetState.renderContext.textureManager, presetState.audioData,\n" +
        "                              presetState.renderContext.frame, presetState.renderContext.fps,\n" +
        "                              presetState.fftAttack, presetState.fftDecay);\n\n" +
        "    m_shader.Bind();\n\n    m_shader.SetUniformMat4x4(\"vertex_transformation\"",
    ],
  ]);
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
patchProjectM(join(src, "src", "libprojectM"));

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
