// Spotiamp+: the Media Library's little pictures, drawn here (Spotiamp+'s
// own, none of Winamp's): grey on see-through, white where the colour goes.
// They're coloured like Winamp coloured its own (Genex.tint): the tree's
// icons and arrows in the item's colours, the sort arrow in the header's,
// the split button's arrow in the button's.
import { canvasOf } from "./genex.js";

/** @param {number} w @param {number} h @param {(ctx: CanvasRenderingContext2D) => void} draw */
function make(w, h, draw) {
  const c = canvasOf(w, h);
  const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d", { willReadFrequently: true }));
  draw(ctx);
  return c;
}

/** rows of a pixel picture: each char a grey level (0-9 → 0-255), space see-through @param {string[]} rows */
function pixels(rows) {
  const h = rows.length;
  const w = Math.max(...rows.map((r) => r.length));
  return make(w, h, (ctx) => {
    const img = ctx.createImageData(w, h);
    rows.forEach((row, y) => {
      [...row].forEach((ch, x) => {
        if (ch === " ") return;
        const v = Math.round((Number(ch) / 9) * 255);
        const i = (y * w + x) * 4;
        img.data[i] = img.data[i + 1] = img.data[i + 2] = v;
        img.data[i + 3] = 255;
      });
    });
    ctx.putImageData(img, 0, 0);
  });
}

/** @type {Record<string, HTMLCanvasElement>} */
const cache = {};
/** @param {string} key @param {() => HTMLCanvasElement} f */
const once = (key, f) => (cache[key] ??= f());

/** the header's sort arrow, 7x8: up for ascending */
export const sortArrow = (/** @type {boolean} */ up) =>
  once(`sort${up}`, () => {
    const rows = ["   4   ", "   4   ", "  444  ", "  434  ", " 44344 ", " 43334 ", "4433344", "4444444"];
    return pixels(up ? rows : [...rows].reverse());
  });

/** the split button's arrow, 6x4 */
export const splitArrow = () => once("split", () => pixels(["555555", "555555", " 5555 ", "  55  "]));

/** a tree item's open and shut arrows (in its picture's place), 16x16 */
export const treeArrow = (/** @type {boolean} */ open) =>
  once(`tree${open}`, () =>
    make(16, 16, (ctx) => {
      const pic = open
        ? pixels(["8888888", "6666666", " 66666 ", " 66666 ", "  666  ", "  666  ", "   6   "])
        : pixels(["3      ", "32     ", "3222   ", "322222 ", "3222   ", "32     ", "3      "]);
      ctx.drawImage(pic, 4, open ? 4 : 5);
    }),
  );

/** 16x16 icons for the tree, drawn with lines and shapes */
const ICONS = {
  /** Favorite Songs: a heart */
  fav: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.fillStyle = "#fff";
    c.beginPath();
    c.moveTo(8, 14);
    c.bezierCurveTo(2, 9.5, 1, 7, 1.5, 5);
    c.bezierCurveTo(2, 2.5, 6, 1.8, 8, 4.8);
    c.bezierCurveTo(10, 1.8, 14, 2.5, 14.5, 5);
    c.bezierCurveTo(15, 7, 14, 9.5, 8, 14);
    c.fill();
  },
  /** Loved songs: a heart's outline */
  loved: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.strokeStyle = "#fff";
    c.lineWidth = 1.6;
    c.beginPath();
    c.moveTo(8, 13.5);
    c.bezierCurveTo(2.5, 9.5, 1.5, 7, 2, 5.2);
    c.bezierCurveTo(2.6, 3, 6, 2.6, 8, 5.4);
    c.bezierCurveTo(10, 2.6, 13.4, 3, 14, 5.2);
    c.bezierCurveTo(14.5, 7, 13.5, 9.5, 8, 13.5);
    c.stroke();
  },
  /** Recently played: a clock */
  recent: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.strokeStyle = "#fff";
    c.lineWidth = 1.5;
    c.beginPath();
    c.arc(8, 8, 6, 0, Math.PI * 2);
    c.stroke();
    c.beginPath();
    c.moveTo(8, 4.5);
    c.lineTo(8, 8.5);
    c.lineTo(11, 10);
    c.stroke();
  },
  /** Most played: rising bars */
  top: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.fillStyle = "#fff";
    c.fillRect(2, 10, 3, 4);
    c.fillRect(6.5, 7, 3, 7);
    c.fillRect(11, 3, 3, 11);
  },
  /** Listening stats: a chart */
  stats: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.strokeStyle = "#fff";
    c.lineWidth = 1.4;
    c.beginPath();
    c.moveTo(2, 2);
    c.lineTo(2, 14);
    c.lineTo(14.5, 14);
    c.stroke();
    c.beginPath();
    c.moveTo(4, 11);
    c.lineTo(7, 7);
    c.lineTo(10, 9);
    c.lineTo(14, 3.5);
    c.stroke();
  },
  /** Local Media: a note */
  local: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.fillStyle = "#fff";
    c.beginPath();
    c.ellipse(5, 12, 3, 2.3, -0.4, 0, Math.PI * 2);
    c.fill();
    c.beginPath();
    c.ellipse(12, 10.5, 3, 2.3, -0.4, 0, Math.PI * 2);
    c.fill();
    c.fillRect(7, 2.5, 1.5, 9.5);
    c.fillRect(14, 1.5, 1.5, 9);
    c.beginPath();
    c.moveTo(7, 2.5);
    c.lineTo(15.5, 1.2);
    c.lineTo(15.5, 4);
    c.lineTo(7, 5.3);
    c.fill();
  },
  /** Radio: an aerial and its waves */
  radio: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.strokeStyle = "#fff";
    c.fillStyle = "#fff";
    c.lineWidth = 1.4;
    c.beginPath();
    c.arc(8, 6, 1.6, 0, Math.PI * 2);
    c.fill();
    c.beginPath();
    c.moveTo(8, 7);
    c.lineTo(5, 15);
    c.moveTo(8, 7);
    c.lineTo(11, 15);
    c.stroke();
    c.beginPath();
    c.arc(8, 6, 4.5, -Math.PI * 0.8, -Math.PI * 0.2);
    c.stroke();
    c.beginPath();
    c.arc(8, 6, 7, -Math.PI * 0.82, -Math.PI * 0.18);
    c.stroke();
  },
  /** a list (a playlist, a list of Spotiamp+'s, a station list) */
  list: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.fillStyle = "#fff";
    for (const y of [3, 7, 11]) {
      c.fillRect(2, y, 2, 2);
      c.fillRect(6, y, 8, 2);
    }
  },
  /** a folder of playlists */
  folder: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.fillStyle = "#fff";
    c.beginPath();
    c.moveTo(1.5, 4);
    c.lineTo(6, 4);
    c.lineTo(7.5, 5.5);
    c.lineTo(14.5, 5.5);
    c.lineTo(14.5, 13);
    c.lineTo(1.5, 13);
    c.fill();
  },
  /** Search: a magnifier */
  search: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.strokeStyle = "#fff";
    c.lineWidth = 1.6;
    c.beginPath();
    c.arc(6.5, 6.5, 4.2, 0, Math.PI * 2);
    c.stroke();
    c.lineWidth = 2.4;
    c.beginPath();
    c.moveTo(9.6, 9.6);
    c.lineTo(14, 14);
    c.stroke();
  },
  /** Pinned: a pin */
  pin: (/** @type {CanvasRenderingContext2D} */ c) => {
    c.fillStyle = "#fff";
    c.strokeStyle = "#fff";
    c.beginPath();
    c.moveTo(5, 2);
    c.lineTo(11, 2);
    c.lineTo(10, 3.5);
    c.lineTo(10, 7);
    c.lineTo(12.5, 9.5);
    c.lineTo(3.5, 9.5);
    c.lineTo(6, 7);
    c.lineTo(6, 3.5);
    c.fill();
    c.lineWidth = 1.4;
    c.beginPath();
    c.moveTo(8, 9.5);
    c.lineTo(8, 15);
    c.stroke();
  },
};

/** @typedef {keyof typeof ICONS} IconName */

/** a tree icon, 16x16 @param {IconName} name */
export const icon = (name) => once(`icon:${name}`, () => make(16, 16, (ctx) => ICONS[name]?.(ctx)));
