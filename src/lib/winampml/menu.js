// Spotiamp+: the Media Library's right-click and button menus as Winamp 5's
// skinned menus drew them (gen_ml skinnedmenuwnd.cpp, colors.cpp): a 1 px
// frame and 2 px of the menu's colour around the items; each item a line as
// high as the bold skin font plus 4 (never less than Windows' menu line, 20
// px), its text in the skin font 31 px in, a 25 px column for a check mark
// with a line down its right; the item under the mouse on a blend of the
// highlight and the menu's colour, framed a shade darker; a separator a line
// from that column on; a sub-menu's arrow at the right; a shortcut after a
// tab, fainter, at the right. Drawn in the window (its text by Windows,
// native.js); a menu that can't fit beside the mouse is moved in. Read for
// Winamp's behaviour; none of its code is used.
import { DT, measure, metrics, paint } from "./native.js";
import { blend } from "./widgets.js";

/**
 * Items as the Library's menus were already written (Tauri's own shape):
 * @typedef {{ text?: string, action?: () => unknown, enabled?: boolean, checked?: boolean, items?: MenuItem[], item?: string }} MenuItem
 * The menu's colours (MLGetSkinColor, MLSO_MENU):
 * @typedef {{ bg: number[], sel: number[], selFrame: number[], text: number[], textSel: number[], textOff: number[], frame: number[], separator: number[] }} MenuColors
 */

const BORDER = 3;
const GUTTER = 25;
/** Windows' own menu line (SM_CYMENU): no item came out lower */
const MENU_LINE = 20;

// Windows' HLS (shlwapi's ColorRGBToHLS and back: 0..240, whole numbers),
// so a colour made darker comes out as it did in Winamp
const idiv = (/** @type {number} */ a, /** @type {number} */ b) => Math.trunc(a / b);
/** @param {number[]} c @returns {[number, number, number]} */
function toHls([r, g, b]) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = idiv((max + min) * 240 + 255, 510);
  if (max === min) return [160, l, 0];
  const d = max - min;
  const s = l <= 120 ? idiv(idiv(max + min, 2) + d * 240, max + min) : idiv(idiv(510 - max - min, 2) + d * 240, 510 - max - min);
  const norm = (/** @type {number} */ v) => idiv(idiv(d, 2) + max * 40 - v * 40, d);
  let h = r === max ? norm(b) - norm(g) : g === max ? 80 + norm(r) - norm(b) : 160 + norm(g) - norm(r);
  if (h < 0) h += 240;
  else if (h > 240) h -= 240;
  return [h, l, s];
}
/** @param {number} h @param {number} l @param {number} s */
function fromHls(h, l, s) {
  if (!s) {
    const v = idiv(l * 255, 240);
    return [v, v, v];
  }
  const m2 = l > 120 ? s + l - idiv(s * l + 120, 240) : idiv((s + 240) * l + 120, 240);
  const m1 = l * 2 - m2;
  const hue = (/** @type {number} */ x) => {
    if (x > 240) x -= 240;
    else if (x < 0) x += 240;
    const v = x < 40 ? m1 + idiv((m2 - m1) * x + 20, 40) : x < 120 ? m2 : x < 160 ? m1 + idiv((m2 - m1) * (160 - x) + 20, 40) : m1;
    return idiv(v * 255 + 120, 240);
  };
  return [hue(h + 80), hue(h), hue(h - 80)];
}
/** MulDiv: rounded, halves away from zero @param {number} a @param {number} b @param {number} c */
const mulDiv = (a, b, c) => {
  const q = (a * b) / c;
  return Math.sign(q) * Math.floor(Math.abs(q) + 0.5);
};
/** ColorAdjustLuma(rgb, n, TRUE): n tenths of a percent lighter (or darker) @param {number[]} c @param {number} n */
function adjustLuma(c, n) {
  const [h, l, s] = toHls(c);
  const nl = Math.max(0, Math.min(240, l + (n > 0 ? mulDiv(240 - l, n, 1000) : mulDiv(l, n, 1000))));
  return fromHls(h, nl, s);
}

/**
 * The colours from the Media Library's sheet (and a modern skin's
 * wasabi.popupmenu.* when it names them): colors.cpp's UpdateColorCache.
 * @param {import("./genex.js").GenexColors} c @param {Partial<Record<string, number[]>>} [skin]
 * @returns {MenuColors}
 */
export function menuColors(c, skin = {}) {
  const same = (/** @type {number[]} */ a, /** @type {number[]} */ b) => a.join() === b.join();
  const frame1 = same(c.wndbg, c.hilite) ? c.hdrbg : c.hilite;
  const bg = skin.bg ?? c.wndbg;
  let selected = skin.sel ?? c.hilite;
  if (same(selected, bg)) selected = c.selbarbg;
  return {
    bg,
    sel: blend(selected, bg, 127),
    selFrame: adjustLuma(selected, -100),
    text: skin.text ?? c.wndfg,
    textSel: skin.textSel ?? c.wndfg,
    textOff: skin.textOff ?? blend(c.wndfg, c.wndbg, 76),
    frame: skin.frame ?? frame1,
    separator: skin.separator ?? frame1,
  };
}

/** one menu level shown */
class Level {
  /** @param {MenuItem[]} items @param {HTMLCanvasElement} canvas */
  constructor(items, canvas) {
    this.items = items;
    this.canvas = canvas;
    this.x = 0;
    this.y = 0;
    this.w = 0;
    this.h = 0;
    /** @type {{ y: number, h: number }[]} */
    this.rows = [];
    this.hot = -1;
    this.seq = 0;
    /** the widest shortcut (" Ctrl+A"), right-aligned in each item */
    this.shortcutCX = 0;
  }
}

export class SkinnedMenu {
  /**
   * @param {HTMLElement} host the window's element the menus go in (covering it)
   * @param {() => { font: import("./native.js").MlFont, colors: MenuColors } | null} look
   * @param {(items: MenuItem[]) => void} [fallback] a menu taller than the window (Windows' own
   *   would go past it; one drawn in it can't): shown by this instead
   */
  constructor(host, look, fallback) {
    this.host = host;
    this.look = look;
    this.fallback = fallback;
    /** @type {Level[]} */
    this.levels = [];
    this.back = document.createElement("div");
    this.back.style.cssText = "position:absolute;inset:0;z-index:20;display:none";
    this.back.addEventListener("pointerdown", (e) => {
      if (e.target === this.back) this.close();
    });
    this.back.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      if (e.target === this.back) this.close();
    });
    host.appendChild(this.back);
    this.onKey = (/** @type {KeyboardEvent} */ e) => this.key(e);
    this.lineH = 19;
    /** @type {ReturnType<typeof setTimeout> | undefined} */
    this.subTimer = undefined;
    /** a sub-menu on its way */
    this.opening = false;
    this.seq = 0;
  }

  get open() {
    return this.levels.length > 0;
  }

  /**
   * opens a menu at a point of the window (CSS px); "up" ends it there (a
   * button's menu above the button, TPM_BOTTOMALIGN)
   * @param {MenuItem[]} items @param {number} x @param {number} y @param {"down" | "up"} [dir]
   */
  async show(items, x, y, dir = "down") {
    this.close();
    const look = this.look();
    if (!look || !items.length) return;
    const seq = ++this.seq;
    const [boldH] = await metrics({ ...look.font, bold: true });
    if (seq !== this.seq) return;
    // GetLineHeight: the bold skin font's height, and 4
    this.lineH = Math.max(MENU_LINE, boldH + 4);
    if (this.fallback && this.tallest(items) > this.host.clientHeight) {
      this.fallback(items);
      return;
    }
    this.back.style.display = "block";
    window.addEventListener("keydown", this.onKey, true);
    await this.push(items, x, y, null, dir);
  }

  /** @param {MenuItem[]} items @param {number} x @param {number} y @param {Level | null} parent @param {"down" | "up"} [dir] */
  async push(items, x, y, parent, dir = "down") {
    const look = this.look();
    if (!look) return;
    const seq = this.seq;
    const canvas = document.createElement("canvas");
    canvas.style.cssText = "position:absolute;image-rendering:pixelated;z-index:21";
    const level = new Level(items, canvas);
    // the items' sizes (OnReflectedMeasureItem: GetTextExtentPoint32 of the
    // text as written, "&"s and all, and of a shortcut after a tab with a
    // space before it; Windows adds a check's width less one)
    const parts = items.filter((i) => i.item !== "Separator").map((i) => split(i.text ?? ""));
    const mains = parts.map((p) => p[0]);
    const cuts = parts.flatMap((p) => (p[1] ? [" " + p[1]] : []));
    const sizes = await measure(look.font, [...mains, ...cuts], true);
    // (closed, or another menu opened, while it was measured)
    if (seq !== this.seq || (parent && !this.levels.includes(parent))) return;
    const textCX = Math.max(0, ...sizes.slice(0, mains.length).map((s) => s[0]));
    level.shortcutCX = Math.max(0, ...sizes.slice(mains.length).map((s) => s[0]));
    const itemW = textCX + level.shortcutCX + 8 + GUTTER + 46 + 14;
    let top = BORDER;
    for (const it of items) {
      const h = it.item === "Separator" ? 7 : this.lineH;
      level.rows.push({ y: top, h });
      top += h;
    }
    level.w = itemW + BORDER * 2;
    level.h = top + BORDER;
    // where it fits: beside the mouse (or the parent item), else moved in
    const W = this.host.clientWidth;
    const H = this.host.clientHeight;
    let lx = x;
    let ly = dir === "up" ? y - level.h : y;
    if (ly < 0 && dir === "up") ly = y;
    if (parent) {
      if (lx + level.w > W) lx = parent.x - level.w + 3;
    } else if (lx + level.w > W) lx = x - level.w;
    if (ly + level.h > H) ly = parent ? H - level.h : y - level.h;
    level.x = Math.max(0, Math.min(lx, W - level.w));
    level.y = Math.max(0, Math.min(ly, H - level.h));
    canvas.width = level.w;
    canvas.height = level.h;
    Object.assign(canvas.style, { left: `${level.x}px`, top: `${level.y}px`, width: `${level.w}px`, height: `${level.h}px` });
    canvas.addEventListener("pointermove", (e) => this.hover(level, e));
    // the mouse gone off the menu: nothing lit there (unless its sub-menu is open)
    canvas.addEventListener("pointerleave", () => {
      if (level.hot < 0 || this.levels.at(-1) !== level) return;
      clearTimeout(this.subTimer);
      level.hot = -1;
      this.draw(level);
    });
    canvas.addEventListener("pointerdown", (e) => e.preventDefault());
    canvas.addEventListener("pointerup", (e) => this.pick(level, e));
    canvas.addEventListener("contextmenu", (e) => e.preventDefault());
    this.back.appendChild(canvas);
    this.levels.push(level);
    await this.draw(level);
  }

  /** the highest the menu or any of its sub-menus is @param {MenuItem[]} items @returns {number} */
  tallest(items) {
    let h = BORDER * 2;
    let sub = 0;
    for (const it of items) {
      h += it.item === "Separator" ? 7 : this.lineH;
      if (it.items?.length) sub = Math.max(sub, this.tallest(it.items));
    }
    return Math.max(h, sub);
  }

  /** @param {Level} level */
  async draw(level) {
    const look = this.look();
    if (!look) return;
    const c = look.colors;
    /** @type {import("./native.js").MlOp[]} */
    const ops = [];
    const f = (/** @type {number} */ x, /** @type {number} */ y, /** @type {number} */ w, /** @type {number} */ h, /** @type {number[]} */ col) => {
      if (w > 0 && h > 0) ops.push({ k: "f", x, y, w, h, c: col });
    };
    const { w, h } = level;
    // the frame: a line, then the menu's colour
    f(0, 0, w, h, c.frame);
    f(1, 1, w - 2, h - 2, c.bg);
    const left = BORDER;
    const right = w - BORDER;
    level.items.forEach((it, i) => {
      const r = level.rows[i];
      if (it.item === "Separator") {
        f(left, r.y, right - left, r.h, c.bg);
        f(left + 23, r.y + Math.floor(r.h / 2), right - left - 23, 1, c.separator);
      } else {
        const hot = i === level.hot && it.enabled !== false;
        const bg = hot ? c.sel : c.bg;
        f(left, r.y, right - left, r.h, bg);
        if (hot) {
          f(left, r.y, right - left, 1, c.selFrame);
          f(left, r.y + r.h - 1, right - left, 1, c.selFrame);
          f(left, r.y, 1, r.h, c.selFrame);
          f(right - 1, r.y, 1, r.h, c.selFrame);
        }
        const fg = it.enabled === false ? c.textOff : hot ? c.textSel : c.text;
        // (Winamp's marks were 16 px pictures: a check in the column, the arrow 1 px from the right)
        const box = r.y + Math.max(0, Math.floor((r.h - 16) / 2));
        if (it.checked) mark(ops, left + 1 + Math.floor((GUTTER - 16) / 2) + 4, box + 5, fg);
        if (it.items) arrow(ops, right - 16 - 1 + 6, box + 4, fg);
        const [main, cut] = split(it.text ?? "");
        const tl = left + GUTTER + 6;
        const tr = right - GUTTER;
        ops.push({ k: "t", x: tl, y: r.y, w: tr - tl, h: r.h, t: label(main), c: fg, f: DT.LEFT | DT.VCENTER | DT.SINGLELINE | DT.END_ELLIPSIS | DT.NOPREFIX | DT.NOCLIP });
        // the shortcut, fainter, from the widest one's start
        if (cut) ops.push({ k: "t", x: tr - level.shortcutCX, y: r.y, w: level.shortcutCX, h: r.h, t: label(cut), c: blend(fg, bg, 192), f: DT.LEFT | DT.VCENTER | DT.SINGLELINE | DT.NOPREFIX | DT.NOCLIP });
      }
      // the column's line, on every item
      f(left + 22, r.y, 1, r.h, c.separator);
    });
    const seq = ++level.seq;
    const image = await paint(w, h, [look.font], ops);
    if (seq !== level.seq) return;
    const ctx = level.canvas.getContext("2d");
    if (ctx && image) ctx.putImageData(image, 0, 0);
  }

  /** @param {Level} level @param {PointerEvent} e */
  hover(level, e) {
    const at = this.rowAt(level, e.offsetY);
    if (at === level.hot) return;
    level.hot = at;
    // a deeper menu shut when another item is pointed at
    const depth = this.levels.indexOf(level);
    while (this.levels.length > depth + 1) this.levels.pop()?.canvas.remove();
    this.draw(level);
    const it = level.items[at];
    if (it?.items && it.enabled !== false) {
      clearTimeout(this.subTimer);
      this.subTimer = setTimeout(() => {
        if (level.hot === at) this.openSub(level, at, false);
      }, 400); // (Windows' menu delay)
    }
  }

  /**
   * an item's sub-menu, beside it (once: a hover's timer and a click can both
   * ask); by a key, its first item lit, as Windows does
   * @param {Level} level @param {number} at @param {boolean} byKey
   */
  async openSub(level, at, byKey) {
    clearTimeout(this.subTimer);
    const it = level.items[at];
    if (!it?.items || this.levels.at(-1) !== level || this.opening) return;
    this.opening = true;
    try {
      await this.push(it.items, level.x + level.w - 3, level.y + level.rows[at].y - BORDER, level);
    } finally {
      this.opening = false;
    }
    const sub = this.levels.at(-1);
    if (byKey && sub && sub !== level) {
      sub.hot = sub.items.findIndex((i) => i.item !== "Separator" && i.enabled !== false);
      this.draw(sub);
    }
  }

  /** @param {Level} level @param {number} y */
  rowAt(level, y) {
    for (let i = 0; i < level.rows.length; i++) {
      const r = level.rows[i];
      if (y >= r.y && y < r.y + r.h) return level.items[i].item === "Separator" ? -1 : i;
    }
    return -1;
  }

  /** @param {Level} level @param {PointerEvent} e */
  pick(level, e) {
    const at = this.rowAt(level, e.offsetY);
    const it = level.items[at];
    if (!it || it.enabled === false) return;
    if (it.items) {
      this.openSub(level, at, false);
      return;
    }
    this.close();
    it.action?.();
  }

  /** @param {KeyboardEvent} e */
  key(e) {
    const level = this.levels.at(-1);
    if (!level) return;
    e.preventDefault();
    e.stopPropagation();
    const items = level.items;
    const live = (/** @type {number} */ i) => items[i] && items[i].item !== "Separator" && items[i].enabled !== false;
    const step = (/** @type {number} */ d) => {
      // (from nothing lit: Down to the first, Up to the last)
      let i = level.hot < 0 ? (d > 0 ? -1 : items.length) : level.hot;
      for (let n = 0; n < items.length; n++) {
        i = (i + d + items.length) % items.length;
        if (live(i)) break;
      }
      level.hot = i;
      this.draw(level);
    };
    if (e.key === "Escape") {
      this.levels.pop()?.canvas.remove();
      if (!this.levels.length) this.close();
    } else if (e.key === "ArrowDown") step(1);
    else if (e.key === "ArrowUp") step(-1);
    else if (e.key === "ArrowLeft" && this.levels.length > 1) this.levels.pop()?.canvas.remove();
    else if ((e.key === "ArrowRight" || e.key === "Enter") && live(level.hot)) {
      const it = items[level.hot];
      if (it.items) this.openSub(level, level.hot, true);
      else if (e.key === "Enter") {
        this.close();
        it.action?.();
      }
    }
  }

  close() {
    this.seq++;
    clearTimeout(this.subTimer);
    for (const l of this.levels) l.canvas.remove();
    this.levels = [];
    this.back.style.display = "none";
    window.removeEventListener("keydown", this.onKey, true);
  }

  destroy() {
    this.close();
    this.back.remove();
  }
}

/** a Windows menu's text: "&&" is an "&" @param {string | undefined} t */
const label = (t) => (t ?? "").replaceAll("&&", "&");
/** an item's text and its shortcut (after the last tab) @param {string} t @returns {[string, string]} */
const split = (t) => {
  const k = t.lastIndexOf("\t");
  return k > 0 ? [t.slice(0, k), t.slice(k + 1)] : [t, ""];
};

/** a check mark, 7x7 and two thick @param {import("./native.js").MlOp[]} ops @param {number} x @param {number} y @param {number[]} c */
function mark(ops, x, y, c) {
  // each row's runs: [row, from, to]
  const runs = [[0, 6, 6], [1, 5, 6], [2, 0, 0], [2, 4, 6], [3, 0, 1], [3, 3, 5], [4, 0, 4], [5, 1, 3], [6, 2, 2]];
  for (const [dy, a, b] of runs) ops.push({ k: "f", x: x + a, y: y + dy, w: b - a + 1, h: 1, c });
}

/** a sub-menu's arrow, 4x7 (Spotiamp+'s own) @param {import("./native.js").MlOp[]} ops @param {number} x @param {number} y @param {number[]} c */
function arrow(ops, x, y, c) {
  for (let i = 0; i < 4; i++) ops.push({ k: "f", x: x + i, y: y + i, w: 1, h: 7 - i * 2, c });
}
