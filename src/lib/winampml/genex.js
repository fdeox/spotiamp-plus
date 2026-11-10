// Spotiamp+: GENEX.BMP, the sheet Winamp 5 drew its Media Library with
// (wa_dlg.h): 24 colours along the top row from x 48, every other pixel; the
// button (47x15, normal at y 0, pressed at y 15, 4 px edges); the scrollbar's
// arrows (14x14 at y 31 and 45) and thumbs (56/70,31 upright, 84,31/45
// across). A classic skin brings its own; one without gets Winamp's (the base
// skin's sheet with 5.666's colours); a modern skin's is made from its own
// pictures and colours (modern/mlgenex.ts, gen_ff's ff_genwa2skinbitmap).
// Read for its behaviour; none of Winamp's code is used.

/** the colours, in the sheet's order */
export const COLOR_NAMES = /** @type {const} */ ([
  "itembg", "itemfg", "wndbg", "buttonfg", "wndfg", "hilite", "selcolor",
  "hdrbg", "hdrfg", "hdrtop", "hdrmid", "hdrbot", "hdrempty",
  "sbfg", "sbbg", "sbinvfg", "sbinvbg", "sbdead",
  "selbarfg", "selbarbg", "iselbarfg", "iselbarbg", "itembg2", "itemfg2",
]);

/** what Winamp took for a colour an old sheet doesn't have (wadlg_defcolors) */
const DEFAULTS = [
  [0, 0, 0], [0, 255, 0], [36, 36, 60], [57, 56, 66], [255, 255, 255], [132, 148, 165], [0, 0, 198],
  [72, 72, 120], [255, 255, 255], [108, 108, 180], [36, 36, 60], [18, 18, 30], [36, 36, 60],
  [36, 36, 60], [36, 36, 60], [121, 130, 150], [78, 88, 110], [36, 36, 60],
  [255, 255, 255], [0, 0, 180], [0, 255, 0], [0, 0, 128], [0, 0, 0], [0, 255, 0],
];

/** Winamp 5.666's own sheet has these where the base skin's 2.91 one has none */
const BASE_5666 = {
  selbarfg: [255, 255, 255],
  selbarbg: [0, 0, 180],
  iselbarfg: [0, 255, 0],
  iselbarbg: [0, 0, 128],
  itembg2: [7, 7, 10],
  itemfg2: [0, 255, 0],
};

// Winamp's own sheets (the base skin's), bundled by Vite so a build finds them
import BASE_GENEX_URL from "../../static/assets/skins/base-2.91/GENEX.BMP?url";
import BASE_GEN_URL from "../../static/assets/skins/base-2.91/GEN.BMP?url";

export const BASE_SHEET = BASE_GENEX_URL;
/** GEN.BMP for a skin without one */
export const BASE_GEN = BASE_GEN_URL;

/** @typedef {Record<typeof COLOR_NAMES[number], number[]>} GenexColors */

/** @param {string} url @returns {Promise<HTMLImageElement | null>} */
export function loadImage(url) {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => resolve(null);
    img.src = url;
  });
}

/** @param {number} w @param {number} h */
export function canvasOf(w, h) {
  const c = document.createElement("canvas");
  c.width = Math.max(1, Math.round(w));
  c.height = Math.max(1, Math.round(h));
  return c;
}

/** @param {HTMLCanvasElement} c */
const ctxOf = (c) => /** @type {CanvasRenderingContext2D} */ (c.getContext("2d", { willReadFrequently: true }));

/**
 * The sheet's colours as WADlg_init read them: a pixel that's missing, the
 * "nothing here" cyan or the sheet's own background (111,0) means an old
 * sheet, and Winamp's defaults stand in.
 * @param {HTMLCanvasElement} sheet
 * @returns {GenexColors}
 */
export function readColors(sheet) {
  const ctx = ctxOf(sheet);
  const px = (/** @type {number} */ x) => {
    if (x >= sheet.width || sheet.height < 1) return null;
    const d = ctx.getImageData(x, 0, 1, 1).data;
    return [d[0], d[1], d[2]];
  };
  const same = (/** @type {number[] | null} */ a, /** @type {number[] | null} */ b) => !!a && !!b && a[0] === b[0] && a[1] === b[1] && a[2] === b[2];
  const defbg = px(111);
  /** @type {any} */
  const out = {};
  COLOR_NAMES.forEach((name, i) => {
    let c = px(48 + i * 2);
    if (!c || same(c, [0, 198, 255]) || same(c, defbg)) {
      if (name === "selbarfg" || name === "iselbarfg") c = out.wndfg;
      else if (name === "selbarbg") c = out.selcolor;
      else if (name === "iselbarbg") c = out.selcolor.map((/** @type {number} */ v, /** @type {number} */ k) => (v >> 1) + (out.wndbg[k] >> 1));
      else if (name === "itembg2") c = out.itembg;
      else if (name === "itemfg2") c = out.itemfg;
      else c = DEFAULTS[i];
    }
    out[name] = c;
  });
  return out;
}

/** @param {HTMLCanvasElement} sheet @param {Partial<GenexColors>} colors */
function writeColors(sheet, colors) {
  const ctx = ctxOf(sheet);
  COLOR_NAMES.forEach((name, i) => {
    const c = colors[name];
    if (!c) return;
    ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
    ctx.fillRect(48 + i * 2, 0, 1, 1);
  });
}

/** "#rrggbb" or "rgb(r, g, b)" → [r, g, b], or null @param {string} v */
export function parseColor(v) {
  const s = v.trim();
  let m = s.match(/^#([0-9a-f]{6})$/i);
  if (m) {
    const hex = m[1];
    return [0, 2, 4].map((i) => parseInt(hex.slice(i, i + 2), 16));
  }
  m = s.match(/^rgba?\(\s*(\d+)[,\s]+(\d+)[,\s]+(\d+)/i);
  if (m) return [+m[1], +m[2], +m[3]];
  return null;
}

/**
 * A CSS variable's picture, as an address that loads: a skin's own come as
 * data: addresses; Spotiamp+'s built-in ones are written relative to the
 * stylesheet that set them ("./TEXT.xxxx.BMP" in a build), which a page
 * would read relative to itself, so they're resolved against that sheet.
 * @param {string} name
 */
export function cssImageUrl(name) {
  const raw = getComputedStyle(document.body).getPropertyValue(name).trim();
  const m = raw.match(/url\((["']?)(.*?)\1\)/);
  if (!m) return null;
  const u = m[2];
  if (!u) return null;
  if (/^(data:|blob:|https?:|asset:|tauri:)/i.test(u) || u.startsWith("/")) return u;
  const sheet = [...document.styleSheets].find((sh) => sh.href && /\/_app\/immutable\/assets\//.test(sh.href));
  try {
    return new URL(u, sheet?.href ?? location.href).href;
  } catch {
    return null;
  }
}

/**
 * A modern skin's scrollbar pictures, when Winamp drew them itself
 * (freeform scrollbars: skins of version 1.3 and up).
 * @typedef {Record<string, HTMLCanvasElement>} Freeform
 */

export class Genex {
  /** @param {HTMLCanvasElement} sheet @param {Freeform | null} [ff] */
  constructor(sheet, ff = null) {
    this.sheet = sheet;
    this.c = readColors(sheet);
    this.ff = ff;
    /** @type {Map<string, HTMLCanvasElement>} */
    this.cache = new Map();
  }

  /** @param {string} key @param {() => HTMLCanvasElement} make */
  memo(key, make) {
    let c = this.cache.get(key);
    if (!c) {
      c = make();
      this.cache.set(key, c);
      if (this.cache.size > 400) this.cache.delete(this.cache.keys().next().value ?? "");
    }
    return c;
  }

  /**
   * The scrollbars' sizes (GetScrollMetric): 14 px, or a freeform skin's
   * own arrow pictures' (an upright bar as wide as its arrow, the arrow as
   * long as it is high).
   */
  barSizes() {
    const v = this.ff?.["wasabi.scrollbar.vertical.left"];
    const h = this.ff?.["wasabi.scrollbar.horizontal.left"];
    return {
      vThick: v?.width || 14,
      vArrow: v?.height || 14,
      hThick: h?.height || 14,
      hArrow: h?.width || 14,
    };
  }

  /** "rgb()" of a colour @param {typeof COLOR_NAMES[number]} name */
  css(name) {
    const c = this.c[name];
    return `rgb(${c[0]},${c[1]},${c[2]})`;
  }

  /**
   * The button's face, its 4 px edges kept and the rest stretched
   * (PushButton_DrawBackground). @param {number} w @param {number} h @param {boolean} pushed
   */
  button(w, h, pushed) {
    return this.memo(`btn:${w}x${h}:${pushed ? 1 : 0}`, () => {
      const out = canvasOf(w, h);
      const ctx = ctxOf(out);
      const y = pushed ? 15 : 0;
      const s = this.sheet;
      blit(ctx, s, 0, y, 4, 4, 0, 0, 4, 4);
      blit(ctx, s, 4, y, 39, 4, 4, 0, w - 8, 4);
      blit(ctx, s, 43, y, 4, 4, w - 4, 0, 4, 4);
      blit(ctx, s, 0, y + 4, 4, 7, 0, 4, 4, h - 8);
      blit(ctx, s, 43, y + 4, 4, 7, w - 4, 4, 4, h - 8);
      blit(ctx, s, 4, y + 4, 39, 7, 4, 4, w - 8, h - 8);
      blit(ctx, s, 0, y + 11, 4, 4, 0, h - 4, 4, 4);
      blit(ctx, s, 4, y + 11, 39, 4, 4, h - 4, w - 8, 4);
      blit(ctx, s, 43, y + 11, 4, 4, w - 4, h - 4, 4, 4);
      return out;
    });
  }

  /**
   * A scrollbar arrow (DrawFrameCtrl): 14x14 off the sheet, stretched to
   * the button, faded halfway into the window's colour when the bar has
   * nothing to scroll.
   * @param {"up" | "down" | "left" | "right"} dir @param {boolean} pressed @param {boolean} inactive
   * @param {number} [w] @param {number} [h]
   */
  arrow(dir, pressed, inactive, w = 14, h = 14) {
    return this.memo(`arrow:${dir}:${pressed ? 1 : 0}:${inactive ? 1 : 0}:${w}x${h}`, () => {
      const out = canvasOf(w, h);
      const ctx = ctxOf(out);
      const ff = this.ff;
      const ffId = { up: "vertical.left", down: "vertical.right", left: "horizontal.left", right: "horizontal.right" }[dir];
      const ffPic = ff?.[`wasabi.scrollbar.${ffId}${pressed ? ".pressed" : ""}`] ?? ff?.[`wasabi.scrollbar.${ffId}`];
      if (ffPic) {
        const bg = ff?.[dir === "up" || dir === "down" ? "v_bg" : "h_bg"];
        if (bg) blit(ctx, bg, 0, 0, bg.width, bg.height, 0, 0, w, h);
        blit(ctx, ffPic, 0, 0, ffPic.width, ffPic.height, 0, 0, w, h);
      } else {
        const x = { up: 0, down: 14, left: 0, right: 14 }[dir] + (pressed ? 28 : 0);
        const y = dir === "up" || dir === "down" ? 31 : 45;
        blit(ctx, this.sheet, x, y, 14, 14, 0, 0, w, h);
      }
      if (inactive) fade(ctx, w, h, this.c.wndbg, 127);
      return out;
    });
  }

  /**
   * The thumb (DrawBlankButton): ends of 4, stretched parts of 5 either
   * side of the 10 px grip, which stays as it is.
   * @param {number} w @param {number} h @param {boolean} vertical @param {boolean} pushed
   */
  thumb(w, h, vertical, pushed) {
    return this.memo(`thumb:${w}x${h}:${vertical ? 1 : 0}:${pushed ? 1 : 0}`, () => {
      const out = canvasOf(w, h);
      const ctx = ctxOf(out);
      const ffPic = this.ff?.[vertical ? "wasabi.scrollbar.vertical.button" : "wasabi.scrollbar.horizontal.button"];
      if (ffPic) {
        const p = (pushed && this.ff?.[`wasabi.scrollbar.${vertical ? "vertical" : "horizontal"}.button.pressed`]) || ffPic;
        blit(ctx, p, 0, 0, p.width, p.height, 0, 0, w, h);
        return out;
      }
      const s = this.sheet;
      const len = vertical ? h : w;
      const middle = Math.floor(len / 2);
      // in the thumb's own direction: [from, length] off the sheet → [to, length]
      /** @param {number} sOff @param {number} sLen @param {number} dOff @param {number} dLen */
      const part = (sOff, sLen, dOff, dLen) => {
        if (dLen <= 0) return;
        if (vertical) blit(ctx, s, pushed ? 70 : 56, 31 + sOff, 14, sLen, 0, dOff, w, dLen);
        else blit(ctx, s, 84 + sOff, pushed ? 45 : 31, sLen, 14, dOff, 0, dLen, h);
      };
      part(0, 4, 0, 4);
      let p = 4;
      const l = middle - 4 - 5;
      if (l > 0) {
        part(4, 5, p, l);
        p += l;
      }
      const m = Math.min(len - 4 - 4, 10);
      if (m > 0) {
        part(9, m, p, m);
        p += m;
      }
      const l2 = len - p - 4;
      if (l2 > 0) part(19, 5, p, l2);
      part(24, 4, len - 4, 4);
      return out;
    });
  }

  /**
   * A grey picture coloured the way gen_ml's first image filter coloured
   * its icons: white is the text colour, black the background.
   * @param {HTMLCanvasElement} mask @param {number[]} bg @param {number[]} fg
   */
  tint(mask, bg, fg) {
    const key = `tint:${maskId(mask)}:${bg.join(",")}:${fg.join(",")}`;
    return this.memo(key, () => {
      const out = canvasOf(mask.width, mask.height);
      const ctx = ctxOf(out);
      const src = ctxOf(mask).getImageData(0, 0, mask.width, mask.height);
      const d = src.data;
      for (let i = 0; i < d.length; i += 4) {
        const a = d[i + 3];
        for (let k = 0; k < 3; k++) {
          const v = fg[k] - (((fg[k] - bg[k]) * (255 - d[i + k])) >> 8);
          d[i + k] = a === 255 ? v : a === 0 ? bg[k] : Math.floor((v * a + Math.floor(((255 - a) * 255 + 127) / 255) * bg[k] + 127) / 255);
        }
        d[i + 3] = 255;
      }
      ctx.putImageData(src, 0, 0);
      return out;
    });
  }
}

/** @type {WeakMap<HTMLCanvasElement, number>} */
const maskIds = new WeakMap();
let nextMaskId = 1;
/** @param {HTMLCanvasElement} c */
function maskId(c) {
  let id = maskIds.get(c);
  if (!id) maskIds.set(c, (id = nextMaskId++));
  return id;
}

/**
 * StretchBlt in COLORONCOLOR: each target pixel takes the source pixel it
 * falls on, no blending.
 * @param {CanvasRenderingContext2D} ctx @param {HTMLCanvasElement | HTMLImageElement} src
 * @param {number} sx @param {number} sy @param {number} sw @param {number} sh
 * @param {number} dx @param {number} dy @param {number} dw @param {number} dh
 */
export function blit(ctx, src, sx, sy, sw, sh, dx, dy, dw, dh) {
  if (dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0) return;
  if (dw === sw && dh === sh) {
    ctx.drawImage(src, sx, sy, sw, sh, dx, dy, dw, dh);
    return;
  }
  // column by column and row by row, so every pixel is one of the source's:
  // the one under the target pixel's middle
  const tmp = canvasOf(dw, sh);
  const t = ctxOf(tmp);
  for (let x = 0; x < dw; x++) t.drawImage(src, sx + Math.floor(((x + 0.5) * sw) / dw), sy, 1, sh, x, 0, 1, sh);
  for (let y = 0; y < dh; y++) ctx.drawImage(tmp, 0, Math.floor(((y + 0.5) * sh) / dh), dw, 1, dx, dy + y, dw, 1);
}

/** Blends an area into a colour (alpha of 255 keeps it). @param {CanvasRenderingContext2D} ctx @param {number} w @param {number} h @param {number[]} bg @param {number} alpha */
function fade(ctx, w, h, bg, alpha) {
  const img = ctx.getImageData(0, 0, w, h);
  const d = img.data;
  const k = Math.floor(((255 - alpha) * 255 + 127) / 255);
  const add = [Math.floor((bg[0] * k + 127) / 255), (bg[1] * k + 127) >> 8, Math.floor((bg[2] * k + 127) / 255)];
  for (let i = 0; i < d.length; i += 4) {
    for (let c = 0; c < 3; c++) d[i + c] = Math.floor((d[i + c] * alpha) / 255) + add[c];
    d[i + 3] = 255;
  }
  ctx.putImageData(img, 0, 0);
}

/** @param {HTMLImageElement | HTMLCanvasElement} img */
export function toCanvas(img) {
  const c = canvasOf(img.width, img.height);
  ctxOf(c).drawImage(img, 0, 0);
  return c;
}

/**
 * The classic skin's sheet: its own GENEX.BMP (--skin-genex), else Winamp's
 * (the base skin's, with 5.666's colours); Spotiamp+'s built-in colourings
 * put their colours and buttons on Winamp's.
 * @returns {Promise<Genex>}
 */
export async function classicGenex() {
  const own = cssImageUrl("--skin-genex");
  if (own) {
    const img = await loadImage(own);
    if (img && img.width >= 47 && img.height >= 31) return new Genex(toCanvas(img));
  }
  const base = await loadImage(BASE_SHEET);
  const sheet = base ? toCanvas(base) : canvasOf(130, 75);
  writeColors(sheet, BASE_5666);
  // a built-in colouring (amber, cherry, emerald): its colours and buttons
  if (document.body.dataset.skin && document.body.dataset.skin !== "custom") {
    const style = getComputedStyle(document.body);
    /** @type {Record<string, typeof COLOR_NAMES[number][]>} */
    const vars = {
      genexitembg: ["itembg", "itembg2"],
      genexitemfg: ["itemfg", "itemfg2", "iselbarfg"],
      genexwndbg: ["wndbg"],
      genexbtntext: ["buttonfg"],
      genexwndtext: ["wndfg", "selbarfg"],
      genexdivider: ["hilite"],
      genexselbg: ["selcolor", "selbarbg"],
      genexhdrbg: ["hdrbg"],
      genexhdrtext: ["hdrfg"],
    };
    /** @type {Partial<GenexColors>} */
    const colors = {};
    for (const [v, names] of Object.entries(vars)) {
      const c = parseColor(style.getPropertyValue(`--skin-${v}`));
      if (c) for (const n of names) colors[n] = c;
    }
    if (colors.selcolor && colors.wndbg) colors.iselbarbg = colors.selcolor.map((v, k) => (v >> 1) + ((colors.wndbg ?? [0, 0, 0])[k] >> 1));
    writeColors(sheet, colors);
    for (const [v, y] of /** @type {[string, number][]} */ ([["--skin-genexbtn", 0], ["--skin-genexbtnp", 15]])) {
      const url = cssImageUrl(v);
      const btn = url && !url.includes("base-2.91") ? await loadImage(url) : null;
      if (btn && btn.width === 47 && btn.height === 15) ctxOf(sheet).drawImage(btn, 0, y);
    }
  }
  return new Genex(sheet);
}

/**
 * A modern skin's sheet as the modern window made it, with its scrollbar
 * pictures when the skin draws its own.
 * @param {{ genex?: string, ff?: Record<string, string> } | null} look
 * @returns {Promise<Genex | null>}
 */
export async function modernGenex(look) {
  if (!look?.genex) return null;
  const img = await loadImage(look.genex);
  if (!img) return null;
  /** @type {Freeform | null} */
  let ff = null;
  if (look.ff && Object.keys(look.ff).length) {
    ff = {};
    for (const [id, url] of Object.entries(look.ff)) {
      const pic = await loadImage(url);
      if (pic) ff[id] = toCanvas(pic);
    }
  }
  return new Genex(toCanvas(img), ff);
}
