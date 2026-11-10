// Spotiamp+: the Media Library's text as Windows wrote it for Winamp 5's
// skinned controls (src-tauri/src/winamp_ml.rs). The page lays out each
// control and lists what it draws in order (fills, the skin's pictures,
// text); Windows draws them in the skin's font. Read for Winamp's
// behaviour; none of its code is used.
import { invoke } from "@tauri-apps/api/core";

/** DrawText's flags, as Winamp passed them */
export const DT = {
  LEFT: 0x0,
  CENTER: 0x1,
  RIGHT: 0x2,
  VCENTER: 0x4,
  BOTTOM: 0x8,
  SINGLELINE: 0x20,
  NOCLIP: 0x100,
  NOPREFIX: 0x800,
  END_ELLIPSIS: 0x8000,
  WORD_ELLIPSIS: 0x40000,
};

/** @typedef {{ face: string, size: number, bold?: boolean }} MlFont */
/** @typedef {{ k: "f", x: number, y: number, w: number, h: number, c: number[] }
 *   | { k: "i", x: number, y: number, w: number, h: number, d: string }
 *   | { k: "t", x: number, y: number, w: number, h: number, t: string, c: number[], f?: number, font?: number, clip?: number[] }} MlOp */

export function inApp() {
  return typeof window !== "undefined" && !!(/** @type {any} */ (window).__TAURI_INTERNALS__);
}

/**
 * Paints an area: what no op covers stays see-through.
 * @param {number} w @param {number} h @param {MlFont[]} fonts @param {MlOp[]} ops
 * @returns {Promise<ImageData | null>}
 */
export async function paint(w, h, fonts, ops) {
  if (!inApp() || w <= 0 || h <= 0) return null;
  /** @type {ArrayBuffer | null} */
  let buffer = null;
  try {
    buffer = await invoke("winamp_ml_paint", { paint: { w, h, fonts, ops } });
  } catch {
    return null;
  }
  if (!buffer || buffer.byteLength < 8) return null;
  const view = new DataView(buffer);
  const pw = view.getInt32(0, true);
  const ph = view.getInt32(4, true);
  if (pw !== w || ph !== h || buffer.byteLength < 8 + w * h * 4) return null;
  return new ImageData(new Uint8ClampedArray(buffer, 8, w * h * 4), w, h);
}

/** @type {Map<string, number[]>} */
const sizeCache = new Map();
/** @type {Map<string, number[]>} */
const metricCache = new Map();

/** @param {MlFont} f */
const fontKey = (f) => `${f.face}|${f.size}|${f.bold ? 1 : 0}`;

/**
 * A font's [height, ascent, average width, external leading].
 * @param {MlFont} font
 */
export async function metrics(font) {
  const key = fontKey(font);
  const known = metricCache.get(key);
  if (known) return known;
  const m = await measureRaw([font], []);
  const out = m?.metrics?.[0] ?? [Math.round(font.size * 1.25), font.size, Math.round(font.size / 2), 0];
  metricCache.set(key, out);
  return out;
}

/**
 * Each text's [width, height] on one line, as DrawText measures it (or, with
 * `extent`, GetTextExtentPoint32: a pixel or two apart now and then).
 * @param {MlFont} font @param {string[]} texts @param {boolean} [extent]
 * @returns {Promise<number[][]>}
 */
export async function measure(font, texts, extent = false) {
  const fk = fontKey(font) + (extent ? "|x" : "");
  const missing = [...new Set(texts.filter((t) => !sizeCache.has(fk + "\u0001" + t)))];
  if (missing.length) {
    const m = await measureRaw([font], missing.map((t) => [t, 0]), extent);
    missing.forEach((t, i) => {
      const s = m?.sizes?.[i];
      // (outside the app: a guess, so the layout still works)
      sizeCache.set(fk + "\u0001" + t, s ?? [Math.round(t.length * font.size * 0.55), Math.round(font.size * 1.25)]);
    });
    if (sizeCache.size > 20000) sizeCache.clear();
  }
  return texts.map((t) => sizeCache.get(fk + "\u0001" + t) ?? [0, 0]);
}

/** @param {MlFont[]} fonts @param {[string, number][]} texts @param {boolean} [extent] */
async function measureRaw(fonts, texts, extent = false) {
  if (!inApp()) return null;
  try {
    return /** @type {{ metrics: number[][], sizes: number[][] }} */ (await invoke("winamp_ml_measure", { measure: { fonts, texts, extent } }));
  } catch {
    return null;
  }
}

/** @type {WeakMap<HTMLCanvasElement, string>} */
const b64Cache = new WeakMap();

/**
 * A picture's pixels for an "i" op (base64 RGBA), kept with the canvas.
 * @param {HTMLCanvasElement} canvas
 */
export function pixels(canvas) {
  const known = b64Cache.get(canvas);
  if (known) return known;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx || !canvas.width || !canvas.height) return "";
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < data.length; i += CHUNK) bin += String.fromCharCode.apply(null, /** @type {any} */ (data.subarray(i, i + CHUNK)));
  const b64 = btoa(bin);
  b64Cache.set(canvas, b64);
  return b64;
}

/** An "i" op for a canvas at x, y. @param {HTMLCanvasElement} canvas @param {number} x @param {number} y @returns {MlOp} */
export function img(canvas, x, y) {
  return { k: "i", x: Math.round(x), y: Math.round(y), w: canvas.width, h: canvas.height, d: pixels(canvas) };
}
