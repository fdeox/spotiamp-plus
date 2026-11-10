// Spotiamp+: a classic skin's frame for a window like Winamp 5's Media
// Library, drawn from GEN.BMP the way Winamp drew its embedded windows
// (draw_embed.cpp): the titlebar's caps and fills with the title in the
// skin's letters, the sides in 29 px pieces, the 24 px pieces above the
// bottom, the bottom's corners and fill, the close button. Inside it the
// window's own room starts 11 px in, 20 down, and leaves 8 right and 14
// below. Read for its behaviour; none of Winamp's code is used.
import { blit, canvasOf } from "./genex.js";

export const FRAME = { left: 11, top: 20, right: 8, bottom: 14 };

/**
 * @param {CanvasRenderingContext2D} ctx the window's canvas, w x h
 * @param {HTMLImageElement | HTMLCanvasElement} gen GEN.BMP
 * @param {number} w @param {number} h
 * @param {string} title
 * @param {number[][] | null} letters [x, width] of A-Z on the sheet
 * @param {boolean} active
 * @param {boolean} closePressed
 */
export function drawGenFrame(ctx, gen, w, h, title, letters, active, closePressed) {
  ctx.imageSmoothingEnabled = false;
  const state = active ? 0 : 21;
  // the titlebar (draw_embed_tbar)
  const glyph = (/** @type {string} */ ch) => {
    const i = ch.toUpperCase().charCodeAt(0) - 65;
    return i >= 0 && i < 26 && letters?.[i] && letters[i][1] ? letters[i] : null;
  };
  const exact = [...title].reduce((s, ch) => s + (glyph(ch)?.[1] ?? 5), 0);
  let textw = exact + 24;
  textw -= textw % 25;
  if (textw > w - 100) textw = w - 100;
  let xp = 0;
  const piece = (/** @type {number} */ sx, /** @type {number} */ width) => {
    ctx.drawImage(gen, sx, state, width, 20, xp, 0, width, 20);
    xp += width;
  };
  piece(0, 25);
  let nt = Math.floor((w - 100 - textw) / 25);
  if (nt) {
    if (nt & 1) piece(104, 12);
    for (let k = nt >> 1; k > 0; k--) piece(104, 25);
  }
  piece(26, 25);
  nt = Math.floor(textw / 25);
  if (nt > 0) {
    let xstart = xp + Math.floor((textw - exact) / 2);
    if (textw !== exact) xstart++;
    for (let k = nt; k > 0; k--) piece(52, 25);
    // the title in the skin's letters (y 88 focused, 96 not)
    let tx = xstart;
    let maxw = textw;
    for (const ch of title) {
      const gl = glyph(ch);
      const cw = gl ? gl[1] : 5;
      if (gl) {
        if (cw > maxw) break;
        ctx.drawImage(gen, gl[0], 88 + (state ? 8 : 0), cw, 7, tx, 4, cw, 7);
      }
      tx += cw;
      maxw -= cw;
    }
  }
  piece(78, 25);
  nt = Math.floor((w - 100 - textw) / 25);
  if (nt) {
    if (nt & 1) piece(104, 13);
    for (let k = nt >> 1; k > 0; k--) piece(104, 25);
  }
  const rest = (w - 100 - textw) % 25;
  if (rest > 0) {
    blit(ctx, gen, 104, state, 25, 20, xp, 0, rest, 20);
    xp += rest;
  }
  piece(130, 25);
  // the close button: drawn when pressed (the titlebar has the normal one)
  if (closePressed) ctx.drawImage(gen, 148, 42, 9, 9, w - 11, 3, 9, 9);
  // the sides (draw_embed)
  let y = Math.floor((h - 20 - 38) / 29);
  let yp = 20;
  while (y-- > 0) {
    ctx.drawImage(gen, 127, 42, 11, 29, 0, yp, 11, 29);
    ctx.drawImage(gen, 139, 42, 8, 29, w - 8, yp, 8, 29);
    yp += 29;
  }
  y = (h - 20 - 38) % 29;
  if (y > 0) {
    blit(ctx, gen, 127, 42, 11, 29, 0, yp, 11, y);
    blit(ctx, gen, 139, 42, 8, 29, w - 8, yp, 8, y);
    yp += y;
  }
  ctx.drawImage(gen, 158, 42, 11, 24, 0, yp, 11, 24);
  ctx.drawImage(gen, 170, 42, 8, 24, w - 8, yp, 8, 24);
  yp += 24;
  ctx.drawImage(gen, 0, 42, 125, 14, 0, yp, 125, 14);
  let x = Math.floor((w - 250) / 25);
  let bx = 125;
  while (x-- > 0) {
    ctx.drawImage(gen, 127, 72, 25, 14, bx, yp, 25, 14);
    bx += 25;
  }
  x = (w - 250) % 25;
  if (x > 0) {
    blit(ctx, gen, 127, 72, 25, 14, bx, yp, x, 14);
    bx += x;
  }
  ctx.drawImage(gen, 0, 57, 125, 14, bx, yp, 125, 14);
}

/** "x:w x:w …" → 26 [x, w] pairs, or null @param {string} value */
export function parseLetters(value) {
  const cells = value.split(" ").filter(Boolean).map((c) => c.split(":").map(Number));
  return cells.length === 26 && cells.every((c) => c.length === 2 && c.every((n) => n >= 0)) ? cells : null;
}

/**
 * GEN.BMP's letters A-Z as Winamp found them (draw_init): along row 90,
 * each starts where the colour at (0,90) stops and ends where it comes back;
 * a letter it can't find has no width (and is written as a 5 px gap).
 * @param {HTMLCanvasElement} sheet
 */
export function scanLetters(sheet) {
  if (sheet.height <= 90 || sheet.width < 1) return null;
  const ctx = /** @type {CanvasRenderingContext2D} */ (sheet.getContext("2d", { willReadFrequently: true }));
  const row = ctx.getImageData(0, 90, sheet.width, 1).data;
  const at = (/** @type {number} */ x) => (row[x * 4] << 16) | (row[x * 4 + 1] << 8) | row[x * 4 + 2];
  const start = at(0);
  const cells = [];
  let pos = 0;
  for (let i = 0; i < 26; i++) {
    while (pos < sheet.width && at(pos) === start) pos++;
    const offset = pos;
    let width = 0;
    let cnt = 0;
    while (pos < sheet.width) {
      const t = at(pos);
      pos++;
      if (t === start) {
        width = cnt;
        break;
      }
      cnt++;
    }
    // (the base skin's sheet Spotiamp+ ships ends right after its Z, where
    // Winamp's own had one more column: the letter up to the edge)
    if (!width && pos >= sheet.width) {
      let k = offset;
      while (k < sheet.width && at(k) !== start && at(k) !== 0) k++;
      if (k - offset > 0 && k - offset <= 8) width = k - offset;
    }
    cells.push([offset, width]);
  }
  return cells;
}

export { canvasOf };
