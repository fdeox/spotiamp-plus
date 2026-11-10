// Spotiamp+: Winamp 5's Media Library controls, drawn and behaving the way
// gen_ml's skinned ones did (skinnedheader.cpp, skinnedlistview.cpp,
// skinnedscrollwnd.cpp, skinnedbutton.cpp, navigation.cpp): each lays itself
// out in the page's pixels and lists what it draws (native.js), the text
// through Windows in the skin's font. Read for Winamp's behaviour; none of
// its code is used.
import { DT, img } from "./native.js";
import { icon, sortArrow, splitArrow, treeArrow } from "./icons.js";

/** @typedef {import("./native.js").MlOp} MlOp */
/** @typedef {import("./genex.js").Genex} Genex */
/** @typedef {{ x: number, y: number, w: number, h: number }} Rect */

const TEXT = DT.LEFT | DT.VCENTER | DT.SINGLELINE | DT.NOPREFIX;

/** What every control draws with: the sheet, the font's height, measures. */
export class Look {
  /** @param {Genex} genex @param {import("./native.js").MlFont} font @param {number} fontH @param {(texts: string[]) => number[][]} measure */
  constructor(genex, font, fontH, measure) {
    this.g = genex;
    this.font = font;
    this.fontH = fontH;
    /** sizes of texts already measured (the page measures before laying out) */
    this.measure = measure;
  }
  /** @param {string} t */
  textW(t) {
    return this.measure([t])[0]?.[0] ?? 0;
  }
}

/** @param {MlOp[]} ops @param {number} x @param {number} y @param {number} w @param {number} h @param {number[]} c */
export function fill(ops, x, y, w, h, c) {
  if (w > 0 && h > 0) ops.push({ k: "f", x, y, w, h, c });
}

/** @param {MlOp[]} ops @param {string} t @param {Rect} r @param {number[]} c @param {number} f @param {Rect} [clip] */
export function text(ops, t, r, c, f, clip) {
  if (!t || r.w <= 0) return;
  ops.push({ k: "t", x: r.x, y: r.y, w: r.w, h: r.h, t, c, f, ...(clip ? { clip: [clip.x, clip.y, clip.w, clip.h] } : {}) });
}

/** the area's part a click is in @param {Rect} r @param {number} x @param {number} y */
export const inside = (r, x, y) => x >= r.x && y >= r.y && x < r.x + r.w && y < r.y + r.h;

/** Windows' MulDiv: a * b / c, rounded @param {number} a @param {number} b @param {number} c */
const mulDiv = (a, b, c) => (c === 0 ? -1 : Math.round((a * b) / c));

/** BlendColors (colors.cpp): alpha of 255 is all `top` @param {number[]} top @param {number[]} bottom @param {number} alpha */
export function blend(top, bottom, alpha) {
  if (alpha > 254) return top;
  if (alpha < 0) return bottom;
  const k = Math.floor(((255 - alpha) * 255 + 127) / 255);
  return [0, 1, 2].map((i) => Math.floor((top[i] * alpha + k * bottom[i] + 127) / 255));
}

// ---------------------------------------------------------------- scrollbar

/**
 * A skinned scrollbar's state: Windows' SCROLLINFO (nMin 0) and what the
 * mouse holds.
 */
export class ScrollBar {
  /** @param {boolean} vertical */
  constructor(vertical) {
    this.vertical = vertical;
    /** last position (nMax), the visible part (nPage), where it is (nPos) */
    this.max = 0;
    this.page = 0;
    this.pos = 0;
    /** @type {Rect} */
    this.rect = { x: 0, y: 0, w: 0, h: 0 };
    /** "up" | "down" | "pageup" | "pagedown" | "thumb" | null */
    this.held = /** @type {string | null} */ (null);
    /** where the insides sit in the top window (the frame's left and top) */
    this.root = { x: 0, y: 0 };
    /** how long its arrows are (14, or a freeform skin's own) */
    this.arrow = 14;
    this.trackPos = -1;
    this.grab = 0;
  }
  /** IsScrollInfoActive */
  get active() {
    return this.page <= this.max && this.max > 0;
  }
  /** the furthest position */
  get end() {
    return Math.max(0, this.max - Math.max(1, this.page) + 1);
  }
  /** @param {number} p */
  set(p) {
    const v = Math.max(0, Math.min(this.end, Math.round(p)));
    const changed = v !== this.pos;
    this.pos = v;
    return changed;
  }
  /** the thumb's [position, size] along the bar (CalcThumbSize) @param {Genex} g */
  thumb(g) {
    const len = this.vertical ? this.rect.h : this.rect.w;
    const A = this.arrow;
    const work = len - A * 2;
    const span = this.max + 1;
    let size = this.page === 0 ? A : mulDiv(this.page, work, span);
    const ff = g.ff?.[this.vertical ? "wasabi.scrollbar.vertical.button" : "wasabi.scrollbar.horizontal.button"];
    if (ff && this.page) size = this.vertical ? ff.height : ff.width;
    // MINTHUMBSIZE_NT4: Windows NT's smallest thumb
    if (size < 9) size = 9;
    let pos = 0;
    if (span > 0) {
      pos = mulDiv(this.pos, work - size, span - Math.max(1, this.page));
      if (this.held === "thumb" && this.trackPos >= 0) pos = this.trackPos;
      pos = Math.max(0, Math.min(work - size, pos));
    }
    return [pos + A, size, work];
  }
  /** @param {MlOp[]} ops @param {Genex} g @param {Rect} [owner] the window it's on (its brush starts at its corner) */
  paint(ops, g, owner) {
    const r = this.rect;
    if (r.w <= 0 || r.h <= 0) return;
    const v = this.vertical;
    // the pattern brush (SetWindowPatternBrush): its origin is the window's
    // right edge and top (left edge and bottom across) in the top window's
    // pixels, set on the window's own drawing, so which pixels take which
    // colour goes by the window's size and where the insides sit
    const o = owner ?? r;
    const parity = (v ? o.w : o.h) + this.root.x + this.root.y;
    const len = v ? r.h : r.w;
    const inactive = !this.active;
    const c = g.c;
    let fg = c.sbfg;
    let bk = c.sbbg;
    if (inactive) {
      fg = blend(fg, c.wndbg, 127);
      bk = blend(bk, c.wndbg, 127);
    }
    /** a part along the bar → its rectangle @param {number} at @param {number} n */
    const part = (at, n) => (v ? { x: r.x, y: r.y + at, w: r.w, h: n } : { x: r.x + at, y: r.y, w: n, h: r.h });
    const track = (/** @type {Rect} */ p, /** @type {boolean} */ pressed) => {
      if (p.w <= 0 || p.h <= 0) return;
      const ffBg = g.ff?.[v ? "v_bg" : "h_bg"];
      if (ffBg) {
        fill(ops, p.x, p.y, p.w, p.h, c.wndbg);
        ops.push(img(stretched(g, ffBg, p.w, p.h, inactive), p.x, p.y));
        return;
      }
      const a = pressed ? c.sbinvfg : fg;
      const b = pressed ? c.sbinvbg : bk;
      if (a.join() === b.join()) fill(ops, p.x, p.y, p.w, p.h, a);
      else ops.push(img(checker(g, p.w, p.h, a, b, (p.x + p.y - parity) & 1), p.x, p.y));
    };
    const A = this.arrow;
    if (len > A * 2) {
      const up = part(0, A);
      const down = part(len - A, A);
      ops.push(img(g.arrow(v ? "up" : "left", this.held === "up", inactive, up.w, up.h), up.x, up.y));
      if (this.active) {
        const [tp, ts] = this.thumb(g);
        track(part(A, tp - A), this.held === "pageup");
        track(part(tp + ts, len - A - tp - ts), this.held === "pagedown");
        const t = part(tp, ts);
        ops.push(img(g.thumb(t.w, t.h, v, this.held === "thumb"), t.x, t.y));
      } else {
        track(part(A, len - A * 2), false);
      }
      ops.push(img(g.arrow(v ? "down" : "right", this.held === "down", inactive, down.w, down.h), down.x, down.y));
    } else {
      const half = Math.floor(len / 2);
      const a = part(0, half);
      const b = part(len - half, half);
      ops.push(img(g.arrow(v ? "up" : "left", this.held === "up", inactive, a.w, a.h), a.x, a.y));
      ops.push(img(g.arrow(v ? "down" : "right", this.held === "down", inactive, b.w, b.h), b.x, b.y));
      if (len - half * 2 > 0) track(part(half, len - half * 2), false);
    }
  }
  /** what a press at x, y on the bar is @param {number} x @param {number} y @param {Genex} g */
  hit(x, y, g) {
    const r = this.rect;
    const at = this.vertical ? y - r.y : x - r.x;
    const len = this.vertical ? r.h : r.w;
    if (at < this.arrow) return "up";
    if (at >= len - this.arrow) return "down";
    if (!this.active) return null;
    const [tp, ts] = this.thumb(g);
    if (at < tp) return "pageup";
    if (at >= tp + ts) return "pagedown";
    return "thumb";
  }
}

/** a picture stretched to w x h, faded when the bar is idle @param {Genex} g @param {HTMLCanvasElement} src @param {number} w @param {number} h @param {boolean} inactive */
function stretched(g, src, w, h, inactive) {
  return g.memo(`str:${src.width}x${src.height}:${w}x${h}:${inactive ? 1 : 0}:${src.dataset.id ?? (src.dataset.id = String(Math.random()))}`, () => {
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d"));
    ctx.imageSmoothingEnabled = false;
    ctx.fillStyle = g.css("wndbg");
    ctx.fillRect(0, 0, w, h);
    ctx.globalAlpha = inactive ? 0.5 : 1;
    ctx.drawImage(src, 0, 0, w, h);
    return c;
  });
}

/** the scrollbar's checked track (an 8x8 brush of alternate pixels) @param {Genex} g @param {number} w @param {number} h @param {number[]} a @param {number[]} b @param {number} phase */
function checker(g, w, h, a, b, phase) {
  return g.memo(`chk:${w}x${h}:${a}:${b}:${phase}`, () => {
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d"));
    const d = ctx.createImageData(w, h);
    // (the brush's 0 bits in the text colour, its 1s in the background's)
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        const col = (x + y + phase) & 1 ? a : b;
        const i = (y * w + x) * 4;
        d.data[i] = col[0];
        d.data[i + 1] = col[1];
        d.data[i + 2] = col[2];
        d.data[i + 3] = 255;
      }
    ctx.putImageData(d, 0, 0);
    return c;
  });
}

/** the Rating column's star, 11x11: full, or faint for an unrated song @param {boolean} full */
function starMask(full) {
  const key = full ? "starFull" : "starFaint";
  const known = /** @type {any} */ (starMask)[key];
  if (known) return known;
  const c = document.createElement("canvas");
  c.width = c.height = 11;
  const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d"));
  const grey = full ? 255 : 120;
  ctx.fillStyle = `rgb(${grey},${grey},${grey})`;
  ctx.beginPath();
  for (let i = 0; i < 10; i++) {
    const a = -Math.PI / 2 + (i * Math.PI) / 5;
    const r = i % 2 ? 2.4 : 5.6;
    const x = 5.5 + Math.cos(a) * r;
    const y = 6 + Math.sin(a) * r;
    if (i) ctx.lineTo(x, y);
    else ctx.moveTo(x, y);
  }
  ctx.closePath();
  ctx.fill();
  return (/** @type {any} */ (starMask)[key] = c);
}

// ---------------------------------------------------------------- button

export class Button {
  /** @param {string} label @param {{ split?: boolean, image?: HTMLCanvasElement, dropdown?: boolean }} [opts] */
  constructor(label, opts = {}) {
    this.label = label;
    this.split = !!opts.split;
    this.dropdown = !!opts.dropdown;
    this.image = opts.image ?? null;
    /** the page's name for it */
    this.id = "";
    this.rect = { x: 0, y: 0, w: 0, h: 0 };
    this.visible = true;
    this.enabled = true;
    this.pushed = false;
    this.splitPushed = false;
    this.hover = false;
  }
  /** GetIdealSize: the text and margins (and the split part) @param {Look} look */
  ideal(look) {
    const [tw, th] = look.measure([this.label])[0] ?? [0, look.fontH];
    let cx = this.label ? tw + 6 + 6 + 2 : 0;
    const cy = (this.label ? th : look.fontH) + 4;
    if (this.dropdown) cx += 19 - 5;
    if (this.split) cx += 19 - 3;
    if (this.image) cx += this.image.width + (cx ? 3 : 12);
    return [cx, Math.max(cy, this.image ? this.image.height + 4 : 0)];
  }
  /** @param {MlOp[]} ops @param {Look} look */
  paint(ops, look) {
    if (!this.visible) return;
    const { x, y, w, h } = this.rect;
    if (w <= 0 || h <= 0) return;
    const g = look.g;
    const pushed = this.pushed;
    const face = g.button(w, h, pushed);
    ops.push(img(face, x, y));
    const fg = this.enabled ? g.c.buttonfg : disabledText(g);
    const clip = { x: x + 6, y: y + 2, w: w - 12, h: h - 4 };
    // where the text goes (PushButton_CalulateRects, DT_CENTER | DT_VCENTER)
    let [tw, th] = this.label ? (look.measure([this.label])[0] ?? [0, 0]) : [0, 0];
    const iw = this.image?.width ?? 0;
    if (iw) tw += this.label ? iw + 3 : iw;
    if (!this.label && this.image) th = this.image.height;
    let room = w;
    if (this.dropdown) room -= 19 - 5;
    else if (this.split) room -= 19 - 3;
    let l = x + Math.floor((room - tw) / 2);
    let t = y + Math.floor((h - th) / 2);
    if (pushed) {
      if (this.dropdown) t++;
      else l++;
    }
    if (t < y + 2) t = y + 2;
    if (l < x + 6) l = x + 6 + (pushed && !this.dropdown ? 1 : 0);
    let lim = x + w - 6;
    if (this.split) lim = x + w - 16 + (pushed ? 1 : 0);
    else if (this.dropdown) lim = x + w - 14;
    if (this.image) {
      const ih = this.image.height;
      const top = th > ih ? t + Math.floor((th - ih) / 2) : t;
      ops.push(img(buttonArrow(g, face, this.image, l - x, top - y, fg, pushed), l, top));
      l += iw + (this.label ? 3 : 0);
    }
    if (this.label) text(ops, this.label, { x: l, y: t, w: Math.max(1, lim - l), h: th }, fg, DT.SINGLELINE | DT.NOPREFIX | DT.WORD_ELLIPSIS | DT.NOCLIP, clip);
    if (this.split || this.dropdown) {
      const arrow = splitArrow();
      let left = x + w - 19;
      if (pushed && !this.dropdown) left++;
      const ax = left + Math.floor((19 - arrow.width) / 2) - ((19 - arrow.width) % 2);
      const ay = y + Math.floor((h - arrow.height) / 2) + 1 + (this.splitPushed ? 1 : 0);
      if (ax > x + 4 && ay > y + 2) ops.push(img(buttonArrow(g, face, arrow, ax - x, ay - y, fg, pushed), ax, ay));
      if (this.split) {
        const bk = buttonBk(g, pushed);
        const c75 = pushed ? blend(g.c.buttonfg, bk, 191) : this.enabled ? blend(g.c.buttonfg, bk, 191) : blend(g.c.buttonfg, bk, 40);
        const c15 = blend(g.c.buttonfg, bk, 40);
        fill(ops, left, y + 3, 1, h - 6, c75);
        fill(ops, left + 1, y + 3, 1, h - 6, c15);
        if (this.splitPushed) fill(ops, left + 1, y + 3, w - 2 - (left + 1 - x), 1, c15);
      }
    }
  }
  /** which part a press is on: "split", "main" or null @param {number} px @param {number} py */
  hit(px, py) {
    if (!this.visible || !inside(this.rect, px, py)) return null;
    if (this.split && px >= this.rect.x + this.rect.w - 19) return "split";
    return "main";
  }
}

/** the button's text colour when it's off (UpdateExtendedColorTable) @param {Genex} g */
function disabledText(g) {
  const bk = buttonBk(g, false);
  let r = 77;
  let fg;
  do {
    r -= 1;
    fg = blend(g.c.buttonfg, bk, r);
  } while (distance(fg, bk) > 70 && r > 0);
  return fg;
}

/** GetColorDistance @param {number[]} a @param {number[]} b */
const distance = (a, b) => Math.abs((1000 * (a[0] - b[0] + (a[1] - b[1]) + (a[2] - b[2]))) / (3 * 255));

/** the button face's average colour (GetButtonBkColor: 7 rows from y 4) @param {Genex} g @param {boolean} pushed */
function buttonBk(g, pushed) {
  const key = `bk:${pushed ? 1 : 0}`;
  const known = /** @type {any} */ (g)[key];
  if (known) return known;
  const ctx = /** @type {CanvasRenderingContext2D} */ (g.sheet.getContext("2d", { willReadFrequently: true }));
  const y = pushed ? 15 : 0;
  let r = 0, gg = 0, b = 0;
  for (let i = 0; i < 7; i++) {
    const d = ctx.getImageData(4, y + 4 + i, 1, 1).data;
    r += d[0];
    gg += d[1];
    b += d[2];
  }
  return (/** @type {any} */ (g)[key] = [Math.floor(r / 7), Math.floor(gg / 7), Math.floor(b / 7)]);
}

/** the split arrow on the face (the button image filter: luma, then the face under it) */
function buttonArrow(/** @type {Genex} */ g, /** @type {HTMLCanvasElement} */ face, /** @type {HTMLCanvasElement} */ arrow, /** @type {number} */ fx, /** @type {number} */ fy, /** @type {number[]} */ fg, /** @type {boolean} */ pushed) {
  return g.memo(`barrow:${face.width}x${face.height}:${fx},${fy}:${fg}:${pushed ? 1 : 0}`, () => {
    const out = document.createElement("canvas");
    out.width = arrow.width;
    out.height = arrow.height;
    const ctx = /** @type {CanvasRenderingContext2D} */ (out.getContext("2d", { willReadFrequently: true }));
    const under = /** @type {CanvasRenderingContext2D} */ (face.getContext("2d", { willReadFrequently: true })).getImageData(fx, fy, arrow.width, arrow.height).data;
    const src = /** @type {CanvasRenderingContext2D} */ (arrow.getContext("2d", { willReadFrequently: true })).getImageData(0, 0, arrow.width, arrow.height);
    const d = src.data;
    const bk = buttonBk(g, pushed);
    for (let i = 0; i < d.length; i += 4) {
      const luma = 255 - Math.floor((d[i] * 30 + d[i + 1] * 59 + d[i + 2] * 11) / 100);
      const a = d[i + 3];
      for (let k = 0; k < 3; k++) {
        const v = fg[k] - (((fg[k] - bk[k]) * luma) >> 8);
        d[i + k] = a === 0 ? under[i + k] : a === 255 ? v : Math.floor((v * a + Math.floor(((255 - a) * 255 + 127) / 255) * under[i + k] + 127) / 255);
      }
      d[i + 3] = 255;
    }
    ctx.putImageData(src, 0, 0);
    return out;
  });
}

// ---------------------------------------------------------------- static

export class Static {
  /** @param {string} label @param {{ vcenter?: boolean, right?: boolean }} [opts] */
  constructor(label, opts = {}) {
    this.label = label;
    this.vcenter = !!opts.vcenter;
    this.right = !!opts.right;
    this.rect = { x: 0, y: 0, w: 0, h: 0 };
    this.visible = true;
  }
  /** @param {Look} look */
  ideal(look) {
    const [tw, th] = look.measure([this.label])[0] ?? [0, look.fontH];
    return [this.label ? tw + 6 : 0, (this.label ? th : look.fontH) + 4];
  }
  /** @param {MlOp[]} ops @param {Look} look */
  paint(ops, look) {
    if (!this.visible || !this.label) return;
    const f = DT.SINGLELINE | DT.NOPREFIX | DT.END_ELLIPSIS | (this.vcenter ? DT.VCENTER : 0) | (this.right ? DT.RIGHT : 0);
    text(ops, this.label, this.rect, look.g.c.wndfg, f, this.rect);
  }
}

// ---------------------------------------------------------------- edit

/** The search box: Windows' edit, its text drawn by Windows too; typing goes to a hidden input the page keeps. */
export class Edit {
  constructor() {
    this.rect = { x: 0, y: 0, w: 0, h: 0 };
    this.visible = true;
    this.value = "";
    this.focused = false;
    this.caretOn = true;
    this.selStart = 0;
    this.selEnd = 0;
    this.scroll = 0;
  }
  /** @param {MlOp[]} ops @param {Look} look */
  paint(ops, look) {
    if (!this.visible) return;
    const g = look.g;
    const { x, y, w, h } = this.rect;
    fill(ops, x, y, w, h, g.c.itembg);
    // (the edit's own margin: its text 3 px in, at its top)
    const left = x + 3 - this.scroll;
    const v = this.value;
    const a = Math.min(this.selStart, this.selEnd);
    const b = Math.max(this.selStart, this.selEnd);
    const clip = { x: x + 1, y, w: w - 2, h };
    const at = (/** @type {number} */ i) => (i <= 0 ? 0 : look.textW(v.slice(0, i)));
    if (this.focused && b > a) {
      const sx = left + at(a);
      const ex = left + at(b);
      text(ops, v.slice(0, a), { x: left, y, w: w * 4, h: look.fontH }, g.c.itemfg, TEXT & ~DT.VCENTER, clip);
      fill(ops, Math.max(sx, x + 1), y, Math.min(ex, x + w - 1) - Math.max(sx, x + 1), look.fontH, g.c.selbarbg);
      text(ops, v.slice(a, b), { x: sx, y, w: w * 4, h: look.fontH }, g.c.selbarfg, TEXT & ~DT.VCENTER, clip);
      text(ops, v.slice(b), { x: ex, y, w: w * 4, h: look.fontH }, g.c.itemfg, TEXT & ~DT.VCENTER, clip);
    } else {
      text(ops, v, { x: left, y, w: w * 4, h: look.fontH }, g.c.itemfg, TEXT & ~DT.VCENTER, clip);
    }
    if (this.focused && this.caretOn && a === b) {
      const cx = left + at(this.selEnd);
      if (cx >= x && cx < x + w) fill(ops, cx, y, 1, look.fontH, g.c.itemfg);
    }
  }
  /** keeps the caret in view @param {Look} look */
  follow(look) {
    const cx = look.textW(this.value.slice(0, this.selEnd));
    const room = this.rect.w - 5;
    if (cx - this.scroll > room) this.scroll = cx - room;
    if (cx - this.scroll < 0) this.scroll = Math.max(0, cx - Math.floor(room / 3));
    if (look.textW(this.value) < room) this.scroll = 0;
  }
  /** the character a click at px falls before @param {number} px @param {Look} look */
  indexAt(px, look) {
    const rel = px - (this.rect.x + 3 - this.scroll);
    let best = 0;
    for (let i = 1; i <= this.value.length; i++) {
      const wPrev = look.textW(this.value.slice(0, i - 1));
      const wNow = look.textW(this.value.slice(0, i));
      if (rel >= (wPrev + wNow) / 2) best = i;
      else break;
    }
    return best;
  }
}

// ---------------------------------------------------------------- tree

/**
 * @typedef {{ key: string, text: string, level: number, icon?: import("./icons.js").IconName, parent?: boolean, open?: boolean }} TreeItem
 */

/** The navigation tree (a Windows tree view the way gen_ml set it up). */
export class Tree {
  constructor() {
    this.rect = { x: 0, y: 0, w: 0, h: 0 };
    /** @type {TreeItem[]} the items shown, in order */
    this.items = [];
    this.selected = /** @type {string | null} */ (null);
    this.focused = false;
    this.top = 0;
    this.bar = new ScrollBar(true);
    this.rowH = 18;
  }
  /** @param {Look} look */
  layout(look) {
    // Navigation_ItemHeight 18, or the font's height + 2 when that's more
    this.rowH = Math.max(18, look.fontH + 2);
    const rows = Math.floor(this.rect.h / this.rowH);
    const need = this.items.length > rows;
    const sz = look.g.barSizes();
    this.bar.arrow = sz.vArrow;
    this.client = { ...this.rect, w: this.rect.w - (need ? sz.vThick : 0) };
    this.bar.rect = need ? { x: this.rect.x + this.rect.w - sz.vThick, y: this.rect.y, w: sz.vThick, h: this.rect.h } : { x: 0, y: 0, w: 0, h: 0 };
    this.bar.max = this.items.length - 1;
    this.bar.page = rows;
    this.top = Math.max(0, Math.min(this.top, Math.max(0, this.items.length - rows)));
    this.bar.pos = this.top;
  }
  /** @param {MlOp[]} ops @param {Look} look */
  paint(ops, look) {
    const g = look.g;
    const c = /** @type {Rect} */ (this.client ?? this.rect);
    fill(ops, c.x, c.y, c.w, c.h, g.c.itembg);
    const rows = Math.ceil(c.h / this.rowH);
    for (let i = 0; i < rows; i++) {
      const item = this.items[this.top + i];
      if (!item) break;
      const y = c.y + i * this.rowH;
      const sel = item.key === this.selected;
      const bg = sel ? (this.focused ? g.c.selbarbg : g.c.iselbarbg) : g.c.itembg;
      const fg = sel ? (this.focused ? g.c.selbarfg : g.c.iselbarfg) : g.c.itemfg;
      if (sel) fill(ops, c.x, y, c.w, Math.min(this.rowH, c.y + c.h - y), bg);
      const ix = c.x + 3 + item.level * 19;
      const pic = item.parent ? treeArrow(!!item.open) : item.icon ? icon(item.icon) : null;
      if (pic && ix < c.x + c.w) ops.push(img(g.tint(pic, bg, fg), ix, y + Math.floor((this.rowH - 16) / 2)));
      const tx = c.x + 24 + item.level * 19;
      text(ops, item.text, { x: tx, y, w: c.x + c.w - tx + 400, h: this.rowH }, fg, TEXT, { x: c.x, y: c.y, w: c.w, h: c.h });
    }
    this.bar.paint(ops, g, this.rect);
  }
  /** the item at a point, and whether it's on its arrow @param {number} px @param {number} py */
  itemAt(px, py) {
    const c = /** @type {Rect} */ (this.client ?? this.rect);
    if (!inside(c, px, py)) return null;
    const i = this.top + Math.floor((py - c.y) / this.rowH);
    const item = this.items[i];
    if (!item) return null;
    const ix = c.x + 3 + item.level * 19;
    return { item, index: i, onArrow: !!item.parent && px >= ix - 2 && px < ix + 18 };
  }
  /** brings item i into view @param {number} i */
  show(i) {
    const rows = Math.max(1, Math.floor(this.rect.h / this.rowH));
    if (i < this.top) this.top = i;
    else if (i >= this.top + rows) this.top = i - rows + 1;
  }
}

// ---------------------------------------------------------------- list view

/**
 * @typedef {{ id: string, title: string, w: number, right?: boolean }} Column
 */

/** A report-style list with its header (a Windows list view, skinned). */
export class ListView {
  constructor() {
    this.rect = { x: 0, y: 0, w: 0, h: 0 };
    this.visible = true;
    /** what an empty list says */
    this.hint = "";
    /** @type {Column[]} */
    this.columns = [];
    this.count = 0;
    /** @type {(i: number) => (string | { t?: string, dim?: boolean, stars?: number })[]} the row's cells, as the columns are (a dim one fainter; stars: a rating) */
    this.cells = () => [];
    /** the playing row (its text in the playlist's "current" colour) */
    this.current = -1;
    this.currentColor = /** @type {number[] | null} */ (null);
    /** the sorted column and its direction (null: none shown) */
    this.sortCol = /** @type {string | null} */ (null);
    this.sortUp = true;
    this.selected = new Set();
    this.focus = -1;
    this.anchor = -1;
    this.focused = false;
    this.alternate = true;
    this.vbar = new ScrollBar(true);
    this.hbar = new ScrollBar(false);
    this.hdrH = 21;
    this.rowH = 19;
    this.pressedHeader = -1;
    /** @type {Rect} */
    this.body = { x: 0, y: 0, w: 0, h: 0 };
  }
  get totalW() {
    return this.columns.reduce((s, c) => s + c.w, 0);
  }
  get top() {
    return this.vbar.pos;
  }
  get rows() {
    return Math.max(0, Math.floor(this.body.h / this.rowH));
  }
  /** @param {Look} look */
  layout(look) {
    this.hdrH = look.fontH + 6;
    this.rowH = look.fontH + 4;
    const { x, y, w, h } = this.rect;
    const sz = look.g.barSizes();
    const VW = sz.vThick;
    const HH = sz.hThick;
    this.vbar.arrow = sz.vArrow;
    this.hbar.arrow = sz.hArrow;
    let needV = false;
    let needH = false;
    for (let k = 0; k < 3; k++) {
      needV = this.count * this.rowH > h - this.hdrH - (needH ? HH : 0);
      needH = this.totalW > w - (needV ? VW : 0);
    }
    const bw = Math.max(0, w - (needV ? VW : 0));
    const bh = Math.max(0, h - this.hdrH - (needH ? HH : 0));
    this.body = { x, y: y + this.hdrH, w: bw, h: bh };
    this.vbar.rect = needV ? { x: x + bw, y, w: VW, h: h - (needH ? HH : 0) } : { x: 0, y: 0, w: 0, h: 0 };
    this.hbar.rect = needH ? { x, y: y + h - HH, w: bw, h: HH } : { x: 0, y: 0, w: 0, h: 0 };
    this.corner = needV && needH ? { x: x + bw, y: y + h - HH, w: VW, h: HH } : null;
    this.vbar.max = this.count - 1;
    this.vbar.page = this.rows;
    this.vbar.set(this.vbar.pos);
    this.hbar.max = this.totalW - 1;
    this.hbar.page = bw;
    this.hbar.set(needH ? this.hbar.pos : 0);
  }
  /** @param {MlOp[]} ops @param {Look} look */
  paint(ops, look) {
    const g = look.g;
    const c = g.c;
    const { x, y } = this.rect;
    const b = this.body;
    const hs = this.hbar.pos;
    const headClip = { x, y, w: b.w, h: this.hdrH };
    // the header (DrawHeaderItem), and the rest of its row
    let cx = x - hs;
    this.columns.forEach((col, i) => {
      this.paintHeaderItem(ops, look, col, { x: cx, y, w: col.w, h: this.hdrH }, i === this.pressedHeader, headClip);
      cx += col.w;
    });
    if (cx < x + b.w) fill(ops, cx, y, x + b.w - cx, this.hdrH, c.hdrempty);
    // the rows
    fill(ops, b.x, b.y, b.w, b.h, c.itembg);
    const rows = Math.ceil(b.h / this.rowH);
    const total = this.totalW;
    for (let r = 0; r < rows; r++) {
      const i = this.top + r;
      if (i >= this.count) break;
      const ry = b.y + r * this.rowH;
      const sel = this.selected.has(i);
      let bg = c.itembg;
      let fg = c.itemfg;
      if (sel) {
        bg = this.focused ? c.selbarbg : c.iselbarbg;
        fg = this.focused ? c.selbarfg : c.iselbarfg;
      } else if (this.alternate && i % 2) {
        bg = c.itembg2;
        fg = c.itemfg2;
      }
      if (i === this.current && this.currentColor) fg = this.currentColor;
      const rowClip = { x: b.x, y: ry, w: b.w, h: Math.min(this.rowH, b.y + b.h - ry) };
      const left = Math.max(b.x, x - hs + 4);
      const right = Math.min(b.x + b.w, x - hs + total);
      fill(ops, left, ry, right - left, rowClip.h, bg);
      const cells = this.cells(i);
      let colX = x - hs;
      this.columns.forEach((col, k) => {
        const cell = cells[k];
        const t = typeof cell === "string" ? cell : cell?.t;
        const visible = colX + col.w > b.x && colX < b.x + b.w;
        if (visible && typeof cell === "object" && cell?.stars !== undefined) {
          // a rating: its stars, or one faint star when there's none
          const n = cell.stars;
          const sy = ry + Math.floor((this.rowH - 11) / 2);
          for (let s = 0; s < Math.max(1, n); s++) {
            const sx = colX + 5 + s * 12;
            if (sx + 11 > colX + col.w) break;
            const pic = g.tint(starMask(n > 0), bg, fg);
            if (sx >= b.x && sx + 11 <= b.x + b.w) ops.push(img(pic, sx, sy));
          }
        } else if (t && visible) {
          const f = TEXT | DT.END_ELLIPSIS | (col.right ? DT.RIGHT : 0);
          const color = typeof cell === "object" && cell.dim ? blend(fg, bg, 80) : fg;
          // (the first column's label runs to 2 px from its edge, the others' to 6)
          const right = k === 0 ? 2 : 6;
          text(ops, t, { x: colX + 6, y: ry, w: col.w - 6 - right, h: this.rowH }, color, f, rowClip);
        }
        colX += col.w;
      });
    }
    this.vbar.paint(ops, g, this.rect);
    this.hbar.paint(ops, g, this.rect);
    if (this.corner) fill(ops, this.corner.x, this.corner.y, this.corner.w, this.corner.h, c.sbdead);
  }
  /** @param {MlOp[]} ops @param {Look} look @param {Column} col @param {Rect} r @param {boolean} pressed @param {Rect} clip */
  paintHeaderItem(ops, look, col, r, pressed, clip) {
    const c = look.g.c;
    const g = look.g;
    if (r.x + r.w <= clip.x || r.x >= clip.x + clip.w) return;
    // pieces clipped to the header, as ExtTextOut and the lines would be
    const part = (/** @type {number} */ px, /** @type {number} */ py, /** @type {number} */ pw, /** @type {number} */ ph, /** @type {number[]} */ color) => {
      const x0 = Math.max(px, clip.x);
      const x1 = Math.min(px + pw, clip.x + clip.w);
      if (x1 > x0) fill(ops, x0, py, x1 - x0, ph, color);
    };
    part(r.x, r.y, r.w, r.h, c.hdrbg);
    const ox = pressed ? 1 : 0;
    const top = pressed ? c.hdrbot : c.hdrtop;
    part(r.x, r.y, r.w, 1, top);
    part(r.x, r.y, 1, r.h, top);
    part(r.x + r.w - 1, r.y, 1, r.h, c.hdrbot);
    part(r.x - 1 + 1, r.y + r.h - 1, r.w, 1, c.hdrbot);
    if (!pressed) {
      part(r.x + r.w - 2, r.y + 1, 1, r.h - 3, c.hdrmid);
      part(r.x + 1, r.y + r.h - 2, r.w - 2, 1, c.hdrmid);
    }
    let textRight = r.x + r.w - 5 + ox;
    if (this.sortCol === col.id) {
      const arrow = sortArrow(this.sortUp);
      const ax = r.x + r.w - arrow.width - 8;
      const ay = r.y + Math.floor((r.h - arrow.height + 1) / 2);
      if (ax > r.x + 16 && ay > r.y + 2) {
        const tinted = g.tint(arrow, c.hdrbg, c.hdrfg);
        if (ax + arrow.width > clip.x && ax < clip.x + clip.w) ops.push(img(tinted, ax, ay));
        textRight = ax - 3;
      }
    }
    if (textRight > r.x + 4 + ox) {
      const tr = { x: r.x + 5 + ox, y: r.y + 2, w: textRight - (r.x + 5 + ox), h: r.h - 4 };
      const tw = look.measure([col.title])[0]?.[0] ?? 0;
      let f = DT.VCENTER | DT.SINGLELINE | DT.NOPREFIX | (col.right ? DT.RIGHT : 0);
      if (tw > tr.w) {
        f = DT.VCENTER | DT.SINGLELINE | DT.NOPREFIX;
        if (tw - tr.w > 6) f |= DT.END_ELLIPSIS;
        text(ops, col.title, tr, c.hdrfg, f, { x: Math.max(clip.x, tr.x), y: clip.y, w: Math.min(clip.x + clip.w, tr.x + tr.w) - Math.max(clip.x, tr.x), h: clip.h });
      } else {
        text(ops, col.title, tr, c.hdrfg, f | DT.NOCLIP, clip);
      }
    }
  }
  /** what's at a point: { header: index, edge?: index } | { row: index } | null @param {number} px @param {number} py */
  hit(px, py) {
    const { x, y } = this.rect;
    if (py >= y && py < y + this.hdrH && px >= x && px < x + this.body.w) {
      let cx = x - this.hbar.pos;
      for (let i = 0; i < this.columns.length; i++) {
        const right = cx + this.columns[i].w;
        if (Math.abs(px - right) <= 3) return { edge: i };
        if (px >= cx && px < right) return { header: i };
        cx = right;
      }
      return { header: -1 };
    }
    if (inside(this.body, px, py)) {
      const i = this.top + Math.floor((py - this.body.y) / this.rowH);
      return { row: i < this.count ? i : -1 };
    }
    return null;
  }
  /** the column at x, and where in it @param {number} px */
  columnAt(px) {
    let cx = this.rect.x - this.hbar.pos;
    for (const col of this.columns) {
      if (px >= cx && px < cx + col.w) return { col, dx: px - cx };
      cx += col.w;
    }
    return null;
  }
  /** brings row i into view @param {number} i */
  show(i) {
    const rows = Math.max(1, this.rows);
    if (i < this.top) this.vbar.set(i);
    else if (i >= this.top + rows) this.vbar.set(i - rows + 1);
  }
  /** a click on row i, with the keys held @param {number} i @param {boolean} ctrl @param {boolean} shift */
  clickRow(i, ctrl, shift) {
    if (i < 0) {
      if (!ctrl && !shift) this.selected.clear();
      return;
    }
    if (shift && this.anchor >= 0) {
      if (!ctrl) this.selected.clear();
      const [a, b] = [Math.min(this.anchor, i), Math.max(this.anchor, i)];
      for (let k = a; k <= b; k++) this.selected.add(k);
    } else if (ctrl) {
      if (this.selected.has(i)) this.selected.delete(i);
      else this.selected.add(i);
      this.anchor = i;
    } else {
      this.selected.clear();
      this.selected.add(i);
      this.anchor = i;
    }
    this.focus = i;
  }
  /** the selected rows, in order */
  selection() {
    return [...this.selected].filter((i) => i < this.count).sort((a, b) => a - b);
  }
}
