// Spotiamp+: the Media Library window's insides the way Winamp 5 laid them
// out and drew them (gen_ml view_ml.cpp, ml_local view_audio.cpp and
// view_media.cpp): the tree with the Library button under it and the
// divider beside it; on the right the search row, the Artist and Album
// filters over the tracks (Local Media) or the tracks alone, the buttons and
// the status line under them. One canvas, painted in one go by Windows'
// text (native.js); the page gives the data and hears what's pressed.
// Read for Winamp's behaviour; none of its code is used.
import { DT, inApp, measure, metrics, paint } from "./native.js";
import { Button, Edit, ListView, Look, ScrollBar, Static, Tree, fill, inside, text } from "./widgets.js";
import { canvasOf } from "./genex.js";

/** @typedef {import("./widgets.js").Column} Column */
/** @typedef {import("./widgets.js").TreeItem} TreeItem */
/** @typedef {import("./native.js").MlOp} MlOp */

/**
 * @typedef {{ columns: Column[], count: number, cells: (i: number) => (string | { t?: string, dim?: boolean, stars?: number })[], current?: number,
 *   sortCol?: string | null, sortUp?: boolean, hint?: string, selected?: number[] }} ListData
 * @typedef {{ id: string, label: string, split?: boolean, dropdown?: boolean, enabled?: boolean }} ButtonData
 * @typedef {{ kind: "audio" | "simple", tracks: ListData, artists?: ListData, albums?: ListData,
 *   buttons: ButtonData[], status: string, filters?: boolean }} Pane
 * @typedef {{
 *   treeSelect: (key: string) => void, treeActivate: (key: string) => void, treeToggle: (key: string) => void,
 *   treeContext: (key: string, e: MouseEvent) => void, treeDrag?: (key: string, e: DragEvent) => boolean,
 *   rowActivate: (i: number) => void, cellClick?: (i: number, col: string, dx: number, starW: number) => void, rowContext: (i: number, e: MouseEvent) => void, rowsDrag: (rows: number[], e: DragEvent) => boolean,
 *   sort: (list: "tracks" | "artists" | "albums", col: string) => void, columnsChanged: (list: string, widths: Record<string, number>) => void,
 *   filterChanged: (artists: number[], albums: number[]) => void, selectionChanged: (rows: number[]) => void,
 *   search: (text: string) => void, searchEnter: (text: string) => void, clear: () => void,
 *   button: (id: string, part: "main" | "split", e: MouseEvent) => void, library: (e: MouseEvent) => void,
 *   toolbar: (id: string, e: MouseEvent) => void, layoutChanged: (state: Record<string, number>) => void,
 * }} Model
 */

const HELD_DELAY = 400;
const HELD_REPEAT = 50;

export class MlView {
  /**
   * @param {HTMLCanvasElement} canvas the insides, at the page's pixels
   * @param {HTMLInputElement} input the hidden field the search box types into
   * @param {Model} model
   * @param {{ ldiv?: number, media?: number, filter?: number }} [state] dividers as last left
   */
  constructor(canvas, input, model, state = {}) {
    this.canvas = canvas;
    this.input = input;
    this.model = model;
    /** @type {Look | null} */
    this.look = null;
    this.w = 0;
    this.h = 0;
    // the dividers: the tree's width, and the filters' height and split (in 1/100000)
    this.ldiv = state.ldiv ?? 135;
    this.mediaPos = state.media ?? 50000;
    this.filterPos = state.filter ?? 50000;
    this.tree = new Tree();
    this.libBtn = new Button("Library");
    this.artBtn = new Button("", { image: toolbarIcon("art") });
    this.viewBtn = new Button("", { image: toolbarIcon("view") });
    this.colsBtn = new Button("", { image: toolbarIcon("cols") });
    this.label = new Static("Search:");
    this.edit = new Edit();
    this.clearBtn = new Button("Clear Search");
    this.artists = new ListView();
    this.albums = new ListView();
    this.tracks = new ListView();
    /** @type {Button[]} */
    this.buttons = [];
    /** @type {{ x: number, y: number, w: number, h: number } | null} the last button pressed */
    this.pressedRect = null;
    this.status = new Static("", { vcenter: true });
    /** @type {Pane | null} */
    this.pane = null;
    /** where the dividers are drawn, and the borders */
    this.lines = /** @type {{ vdelim: any, div1: any, hdelim: any, borders: any[] }} */ ({ vdelim: null, div1: null, hdelim: null, borders: [] });
    /** what the mouse holds: a button, a divider, a scrollbar, a column edge */
    this.drag = /** @type {any} */ (null);
    this.focus = /** @type {"tree" | "tracks" | "artists" | "albums" | "edit" | null} */ (null);
    this.divX = 0;
    this.divY = 0;
    /** @type {{ x: number, y: number, w: number, h: number }} */
    this.treeBox = { x: 0, y: 0, w: 0, h: 0 };
    /** @type {{ x: number, y: number, w: number, h: number }} */
    this.view = { x: 0, y: 0, w: 0, h: 0 };
    this.active = true;
    this.dirty = true;
    this.painting = false;
    this.again = false;
    this.frame = 0;
    this.caretTimer = 0;
    this.heldTimer = 0;
    this.dragArm = /** @type {any} */ (null);
    this.wire();
  }

  // ------------------------------------------------------------ data in

  /** @param {Look} look */
  setLook(look) {
    this.look = look;
    this.invalidate();
  }
  /** @param {number} w @param {number} h */
  setSize(w, h) {
    w = Math.max(0, Math.round(w));
    h = Math.max(0, Math.round(h));
    if (w === this.w && h === this.h) return;
    this.w = w;
    this.h = h;
    this.invalidate();
  }
  /** where the insides sit in the window (the frame's room) @param {number} x @param {number} y */
  setRoot(x, y) {
    for (const bar of [this.tree.bar, this.tracks.vbar, this.tracks.hbar, this.artists.vbar, this.artists.hbar, this.albums.vbar, this.albums.hbar]) bar.root = { x, y };
    this.invalidate();
  }
  /** @param {TreeItem[]} items @param {string | null} selected */
  setTree(items, selected) {
    this.tree.items = items;
    this.tree.selected = selected;
    this.invalidate();
  }
  /** @param {Pane | null} pane */
  setPane(pane) {
    const was = this.pane;
    this.pane = pane;
    if (!pane) return this.invalidate();
    if (pane.kind !== was?.kind) this.tracks.vbar.pos = 0;
    this.fill(this.tracks, pane.tracks);
    if (pane.artists) this.fill(this.artists, pane.artists);
    if (pane.albums) this.fill(this.albums, pane.albums);
    // the buttons, kept by id so a pressed one stays pressed
    const old = new Map(this.buttons.map((b) => [b.id, b]));
    this.buttons = pane.buttons.map((d) => {
      const b = /** @type {Button & { id: string }} */ (old.get(d.id) ?? Object.assign(new Button(d.label, { split: d.split, dropdown: d.dropdown }), { id: d.id }));
      b.label = d.label;
      b.split = !!d.split;
      b.dropdown = !!d.dropdown;
      b.enabled = d.enabled !== false;
      return b;
    });
    this.status.label = pane.status;
    this.invalidate();
  }
  /** @param {ListView} list @param {ListData} data */
  fill(list, data) {
    const sameCount = list.count === data.count;
    list.columns = data.columns;
    list.count = data.count;
    list.cells = data.cells;
    list.current = data.current ?? -1;
    list.sortCol = data.sortCol ?? null;
    list.sortUp = data.sortUp ?? true;
    list.hint = data.hint ?? "";
    if (data.selected) {
      list.selected = new Set(data.selected.filter((i) => i < data.count));
      if (list.focus < 0 || list.focus >= data.count) list.focus = data.selected[0] ?? -1;
    } else if (!sameCount) {
      for (const i of [...list.selected]) if (i >= data.count) list.selected.delete(i);
      if (list.focus >= data.count) list.focus = -1;
    }
  }
  /** the search box's text, from the page @param {string} value */
  setSearch(value) {
    if (this.edit.value === value) return;
    this.edit.value = value;
    this.edit.selStart = this.edit.selEnd = Math.min(this.edit.selEnd, value.length);
    this.invalidate();
  }
  /** a row picked from outside (type-to-find) @param {number} i */
  selectRow(i) {
    this.tracks.clickRow(i, false, false);
    this.tracks.show(i);
    this.model.selectionChanged(this.tracks.selection());
    this.invalidate();
  }
  /** every row (the menu's Select all) */
  selectAll() {
    for (let i = 0; i < this.tracks.count; i++) this.tracks.selected.add(i);
    this.listChanged("tracks");
    this.invalidate();
  }
  clearSelection() {
    this.tracks.selected.clear();
    this.tracks.focus = -1;
    this.tracks.anchor = -1;
    this.invalidate();
  }
  /** @param {boolean} active the window in front */
  setActive(active) {
    this.active = active;
    if (!active) this.tree.focused = this.tracks.focused = this.artists.focused = this.albums.focused = false;
    else this.applyFocus();
    this.invalidate();
  }

  // ------------------------------------------------------------ layout

  layout() {
    const look = this.look;
    if (!look || !this.w || !this.h) return;
    const fh = look.fontH;
    const btnH = fh + 4;
    const W = this.w;
    const bottom = this.h > 2 ? this.h - 2 : this.h;
    // view_ml.cpp: the tree, the Library button under it, the divider
    let divX = this.ldiv;
    if (divX > W - 41) divX = W - 9;
    if (divX < 32) divX = 0;
    else if (divX > W) divX = W;
    this.divX = divX;
    this.libBtn.rect = { x: 0, y: bottom - btnH, w: divX, h: btnH };
    this.libBtn.visible = divX > 0;
    const treeH = Math.max(0, bottom - btnH - 3);
    this.tree.rect = { x: 0, y: 0, w: Math.max(0, divX - 1), h: Math.max(0, treeH - 1) };
    this.treeBox = { x: 0, y: 0, w: divX, h: treeH };
    this.tree.layout(look);
    this.lines.vdelim = { x: divX + 1, y: 0, w: 9, h: bottom };
    const x0 = divX + 9;
    const vw = Math.max(0, W - x0 - 2);
    this.view = { x: x0, y: 0, w: vw, h: bottom };
    const pane = this.pane;
    this.lines.borders = [];
    this.lines.div1 = null;
    this.lines.hdelim = null;
    if (!pane) return;
    const audio = pane.kind === "audio";
    // the search row (GROUP_SEARCH)
    const clearIdeal = this.clearBtn.ideal(look);
    let rg = audio
      ? { left: x0 + 1, top: 2, right: x0 + vw - 2, bottom: clearIdeal[1] + 1 }
      : { left: x0, top: 2, right: x0 + vw - 2, bottom: clearIdeal[1] + 1 };
    const rowH = rg.bottom - rg.top;
    const tools = audio ? [this.artBtn, this.viewBtn, this.colsBtn] : [];
    const toolW = [21, 30, 30];
    for (const b of [this.artBtn, this.viewBtn, this.colsBtn]) b.visible = false;
    tools.forEach((b, i) => {
      b.visible = true;
      b.rect = { x: rg.left, y: rg.top, w: toolW[i], h: rowH };
      rg.left += toolW[i] + 1;
    });
    const labelIdeal = this.label.ideal(look);
    this.label.rect = { x: rg.left + (audio ? 6 : 2), y: rg.top + 1, w: labelIdeal[0], h: rowH };
    rg.left += labelIdeal[0] + (audio ? 8 : 4);
    const clearW = clearIdeal[0] + 6;
    this.clearBtn.visible = rg.right - rg.left - clearW > 40;
    this.clearBtn.rect = { x: rg.right - clearW, y: rg.top, w: clearW, h: rowH };
    if (this.clearBtn.visible) rg.right -= clearW + 4;
    this.edit.visible = rg.right > rg.left;
    this.edit.rect = { x: rg.left, y: rg.top, w: Math.max(0, rg.right - rg.left - 1), h: rowH - 1 };
    if (this.edit.visible) this.lines.borders.push(this.edit.rect);
    let top = rg.bottom + 3;
    // the filters over the tracks (view_audio.cpp's LayoutWindows)
    let mediaTop = top;
    this.artists.visible = this.albums.visible = false;
    if (audio && pane.filters !== false) {
      let divY = Math.floor((this.mediaPos * bottom) / 100000);
      const hdelimH = 10;
      let filters = true;
      if (divY > bottom - 85) divY = bottom - hdelimH;
      else if (divY < top + 24) {
        divY = top;
        filters = false;
      }
      if (filters) {
        let divX1 = Math.floor((this.filterPos * vw) / 100000);
        if (divX1 < 15) divX1 = 0;
        else if (divX1 > vw - 27) divX1 = vw - 8;
        const fy = top + 1;
        const fh2 = divY - top - 1;
        this.artists.visible = divX1 > 0;
        this.artists.rect = { x: x0 + 1, y: fy, w: divX1, h: fh2 };
        const d1x = x0 + 1 + divX1;
        this.lines.div1 = { x: d1x, y: fy, w: 8, h: divY - top };
        const ax = d1x + 8;
        this.albums.visible = true;
        this.albums.rect = { x: ax, y: fy, w: Math.max(0, x0 + vw - ax - 3), h: fh2 };
        if (this.artists.visible) this.lines.borders.push(this.artists.rect);
        this.lines.borders.push(this.albums.rect);
        this.artists.layout(look);
        this.albums.layout(look);
      }
      this.lines.hdelim = { x: x0, y: divY, w: vw - 2, h: hdelimH, show: filters };
      this.divY = divY;
      mediaTop = divY + hdelimH;
    }
    // the tracks, the buttons and the status (view_media.cpp)
    const mh = bottom - mediaTop;
    const bx0 = x0 + 1;
    let bl = bx0;
    const by = mediaTop + mh - btnH;
    for (const b of this.buttons) {
      const [iw] = b.ideal(look);
      const w = iw + 6;
      b.rect = { x: bl, y: by, w, h: btnH };
      b.visible = x0 + vw - bl > w;
      if (b.visible) bl += w + 4;
    }
    this.status.rect = { x: bl, y: by, w: Math.max(0, x0 + vw - bl), h: btnH };
    this.status.visible = this.status.rect.w > 16;
    const listBottom = by - 3;
    const lt = audio ? mediaTop + 1 : top + 1;
    this.tracks.rect = { x: x0 + 1, y: lt, w: Math.max(0, vw - 1 - 3), h: Math.max(0, listBottom - lt - (audio ? 1 : 1)) };
    this.tracks.layout(look);
    this.lines.borders.push(this.tracks.rect);
  }

  // ------------------------------------------------------------ painting

  invalidate() {
    this.dirty = true;
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.render();
    });
  }

  async render() {
    if (!this.dirty) return;
    if (this.painting) {
      this.again = true;
      return;
    }
    const look = this.look;
    if (!look || !this.w || !this.h) return;
    this.dirty = false;
    this.painting = true;
    try {
      this.layout();
      const ops = this.ops(look);
      const w = this.w;
      const h = this.h;
      const image = inApp() ? await paint(w, h, [look.font], ops) : null;
      if (this.canvas.width !== w || this.canvas.height !== h) {
        this.canvas.width = w;
        this.canvas.height = h;
      }
      const ctx = this.canvas.getContext("2d");
      if (ctx && image) ctx.putImageData(image, 0, 0);
      else if (ctx) drawFallback(ctx, ops, look);
      this.placeInput();
    } finally {
      this.painting = false;
      if (this.again) {
        this.again = false;
        this.dirty = true;
        this.render();
      }
    }
  }

  /** @param {Look} look */
  ops(look) {
    /** @type {MlOp[]} */
    const ops = [];
    const c = look.g.c;
    fill(ops, 0, 0, this.w, this.h, c.wndbg);
    // the tree, its border on its right and bottom
    if (this.divX > 0) {
      this.tree.paint(ops, look);
      const b = /** @type {any} */ (this.treeBox);
      fill(ops, b.x + b.w - 1, b.y, 1, b.h, c.hilite);
      fill(ops, b.x, b.y + b.h - 1, b.w, 1, c.hilite);
      this.libBtn.paint(ops, look);
    }
    const vd = this.lines.vdelim;
    if (vd) fill(ops, vd.x + Math.floor(vd.w / 2) - 1, vd.y, 1, vd.h, c.hilite);
    const pane = this.pane;
    if (pane) {
      for (const b of [this.artBtn, this.viewBtn, this.colsBtn]) b.paint(ops, look);
      this.label.paint(ops, look);
      this.edit.paint(ops, look);
      this.clearBtn.paint(ops, look);
      if (this.artists.visible) this.artists.paint(ops, look);
      if (this.albums.visible) this.albums.paint(ops, look);
      const d1 = this.lines.div1;
      if (d1) fill(ops, d1.x + Math.floor(d1.w / 2) - 1, d1.y, 1, d1.h + 1, c.hilite);
      const hd = this.lines.hdelim;
      if (hd?.show) fill(ops, hd.x, hd.y + Math.floor(hd.h / 2), hd.w + 1, 1, c.hilite);
      this.tracks.paint(ops, look);
      const hint = this.tracks.hint;
      if (hint && this.tracks.count === 0) {
        const b = this.tracks.body;
        text(ops, hint, { x: b.x + 8, y: b.y + 6, w: b.w - 16, h: look.fontH * 3 }, c.itemfg, DT.CENTER | DT.NOPREFIX | DT.WORD_ELLIPSIS, b);
      }
      for (const b of this.buttons) b.paint(ops, look);
      if (this.status.visible) this.status.paint(ops, look);
      // the sunken borders on each list's and the box's right and bottom
      for (const r of this.lines.borders) {
        if (r.w <= 0 || r.h <= 0) continue;
        fill(ops, r.x, r.y + r.h, r.w + 1, 1, c.hilite);
        fill(ops, r.x + r.w, r.y, 1, r.h + 1, c.hilite);
      }
    }
    return ops;
  }

  /** the hidden field over the search box, so a click there types into it */
  placeInput() {
    const r = this.edit.rect;
    const s = this.input.style;
    s.left = `${this.canvas.offsetLeft + r.x}px`;
    s.top = `${this.canvas.offsetTop + r.y}px`;
    s.width = `${Math.max(0, r.w)}px`;
    s.height = `${Math.max(0, r.h)}px`;
    s.display = this.edit.visible && this.pane ? "block" : "none";
  }

  // ------------------------------------------------------------ input

  /** the page's pixels of an event @param {MouseEvent} e */
  at(e) {
    const r = this.canvas.getBoundingClientRect();
    const sx = r.width ? this.canvas.width / r.width : 1;
    const sy = r.height ? this.canvas.height / r.height : 1;
    return [(e.clientX - r.left) * sx, (e.clientY - r.top) * sy];
  }

  wire() {
    const cv = this.canvas;
    /** @param {string} type @param {any} f @param {any} [opts] */
    const on = (type, f, opts) => cv.addEventListener(type, f, opts);
    on("pointerdown", (/** @type {PointerEvent} */ e) => this.down(e));
    on("pointermove", (/** @type {PointerEvent} */ e) => this.move(e));
    on("pointerup", (/** @type {PointerEvent} */ e) => this.up(e));
    on("pointercancel", (/** @type {PointerEvent} */ e) => this.up(e));
    on("lostpointercapture", () => this.release());
    on("dblclick", (/** @type {MouseEvent} */ e) => this.dbl(e));
    on("contextmenu", (/** @type {MouseEvent} */ e) => this.context(e));
    on("wheel", (/** @type {WheelEvent} */ e) => this.wheel(e), { passive: false });
    on("dragstart", (/** @type {DragEvent} */ e) => this.dragStart(e));
    on("keydown", (/** @type {KeyboardEvent} */ e) => this.key(e));
    const inp = this.input;
    inp.addEventListener("input", () => this.syncEdit(true));
    inp.addEventListener("keyup", () => this.syncEdit(false));
    inp.addEventListener("select", () => this.syncEdit(false));
    inp.addEventListener("pointerup", () => this.syncEdit(false));
    inp.addEventListener("focus", () => {
      this.focus = "edit";
      this.applyFocus();
      this.blink();
    });
    inp.addEventListener("blur", () => {
      if (this.focus === "edit") this.focus = null;
      this.edit.focused = false;
      clearInterval(this.caretTimer);
      this.invalidate();
    });
    inp.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        this.model.searchEnter(inp.value);
      } else if (e.key === "Escape") {
        inp.blur();
        cv.focus();
      }
      requestAnimationFrame(() => this.syncEdit(false));
    });
  }

  /** @param {boolean} changed */
  syncEdit(changed) {
    const inp = this.input;
    this.edit.value = inp.value;
    this.edit.selStart = inp.selectionStart ?? inp.value.length;
    this.edit.selEnd = inp.selectionEnd ?? inp.value.length;
    if (inp.selectionDirection === "backward") [this.edit.selStart, this.edit.selEnd] = [this.edit.selEnd, this.edit.selStart];
    if (this.look) this.edit.follow(this.look);
    this.edit.caretOn = true;
    if (changed) this.model.search(inp.value);
    this.invalidate();
  }

  blink() {
    clearInterval(this.caretTimer);
    this.edit.caretOn = true;
    this.caretTimer = /** @type {any} */ (setInterval(() => {
      this.edit.caretOn = !this.edit.caretOn;
      this.invalidate();
    }, 530));
  }

  applyFocus() {
    const f = this.active === false ? null : this.focus;
    this.tree.focused = f === "tree";
    this.tracks.focused = f === "tracks";
    this.artists.focused = f === "artists";
    this.albums.focused = f === "albums";
    this.edit.focused = f === "edit";
    this.invalidate();
  }

  /** @param {"tree" | "tracks" | "artists" | "albums" | "edit"} f */
  setFocus(f) {
    this.focus = f;
    if (f !== "edit") {
      if (document.activeElement === this.input) this.input.blur();
      this.canvas.focus({ preventScroll: true });
    }
    this.applyFocus();
  }

  /** the scrollbar under a point, of the tree or a list @param {number} x @param {number} y */
  barAt(x, y) {
    for (const [owner, bar] of /** @type {[any, ScrollBar][]} */ ([
      ["tree", this.tree.bar],
      ["tracks", this.tracks.vbar],
      ["tracks", this.tracks.hbar],
      ["artists", this.artists.vbar],
      ["artists", this.artists.hbar],
      ["albums", this.albums.vbar],
      ["albums", this.albums.hbar],
    ])) {
      if ((owner === "artists" && !this.artists.visible) || (owner === "albums" && !this.albums.visible)) continue;
      if (bar.rect.w > 0 && inside(bar.rect, x, y)) return { owner, bar };
    }
    return null;
  }

  /** the list a point is on @param {number} x @param {number} y */
  listAt(x, y) {
    if (inside(this.tracks.rect, x, y)) return /** @type {const} */ (["tracks", this.tracks]);
    if (this.artists.visible && inside(this.artists.rect, x, y)) return /** @type {const} */ (["artists", this.artists]);
    if (this.albums.visible && inside(this.albums.rect, x, y)) return /** @type {const} */ (["albums", this.albums]);
    return null;
  }

  /** @param {PointerEvent} e */
  down(e) {
    if (e.button !== 0 || !this.look) return;
    const [x, y] = this.at(e);
    this.dragArm = null;
    this.canvas.draggable = false;
    // the dividers
    const vd = this.lines.vdelim;
    if (vd && inside(vd, x, y)) return this.grab(e, { kind: "ldiv", off: x - this.divX });
    const d1 = this.lines.div1;
    if (d1 && inside(d1, x, y)) return this.grab(e, { kind: "div1", off: x - d1.x });
    const hd = this.lines.hdelim;
    if (hd && inside({ ...hd, w: hd.w + 2 }, x, y)) return this.grab(e, { kind: "hdelim", off: y - hd.y });
    // a scrollbar
    const sb = this.barAt(x, y);
    if (sb) {
      const part = sb.bar.hit(x, y, /** @type {Look} */ (this.look).g);
      if (!part) return;
      if (sb.owner !== "tree") this.setFocus(sb.owner);
      sb.bar.held = part;
      if (part === "thumb") {
        const [tp] = sb.bar.thumb(/** @type {Look} */ (this.look).g);
        sb.bar.trackPos = tp - sb.bar.arrow;
        sb.bar.grab = (sb.bar.vertical ? y - sb.bar.rect.y : x - sb.bar.rect.x) - tp;
      } else {
        this.step(sb.owner, sb.bar, part);
        this.heldTimer = /** @type {any} */ (setTimeout(() => {
          this.heldTimer = /** @type {any} */ (setInterval(() => this.step(sb.owner, sb.bar, part, x, y), HELD_REPEAT));
        }, HELD_DELAY));
      }
      this.invalidate();
      return this.grab(e, { kind: "bar", ...sb });
    }
    // the buttons
    const buttons = [this.libBtn, this.artBtn, this.viewBtn, this.colsBtn, this.clearBtn, ...this.buttons];
    for (const b of buttons) {
      const part = b.enabled ? b.hit(x, y) : null;
      if (!part) continue;
      b.pushed = part === "main" || !!b.dropdown;
      b.splitPushed = part === "split";
      if (part === "split") b.pushed = false;
      this.invalidate();
      // a split part or a drop-down opens its menu at once
      if (part === "split" || b.dropdown) {
        this.press(b, part, e);
        b.pushed = b.splitPushed = false;
        this.invalidate();
        return;
      }
      return this.grab(e, { kind: "button", button: b, part });
    }
    // the search box
    if (this.edit.visible && inside(this.edit.rect, x, y)) return;
    // the tree
    if (this.divX > 0 && inside(this.tree.rect, x, y)) {
      this.setFocus("tree");
      const hit = this.tree.itemAt(x, y);
      if (!hit) return;
      if (hit.onArrow) {
        this.model.treeToggle(hit.item.key);
        return;
      }
      this.tree.selected = hit.item.key;
      this.invalidate();
      this.model.treeSelect(hit.item.key);
      if (this.model.treeDrag) {
        this.dragArm = { kind: "tree", key: hit.item.key };
        this.canvas.draggable = true;
      }
      return;
    }
    // a list
    const hitList = this.listAt(x, y);
    if (hitList) {
      const [name, list] = hitList;
      this.setFocus(name);
      const h = list.hit(x, y);
      if (!h) return;
      if ("edge" in h) {
        const col = list.columns[/** @type {number} */ (h.edge)];
        return this.grab(e, { kind: "column", list, name, col, start: x, w0: col.w });
      }
      if ("header" in h) {
        const hi = h.header ?? -1;
        if (hi >= 0) {
          list.pressedHeader = hi;
          this.invalidate();
          return this.grab(e, { kind: "header", list, name, index: hi });
        }
        return;
      }
      const i = /** @type {number} */ (h.row);
      if (i >= 0 && list.selected.has(i) && !e.ctrlKey && !e.shiftKey && name === "tracks") {
        // a press on the selection: it may be dragged; a click alone picks one
        this.dragArm = { kind: "rows", rows: list.selection(), click: i };
        this.canvas.draggable = true;
        return;
      }
      list.clickRow(i, e.ctrlKey, e.shiftKey);
      this.listChanged(name);
      if (i >= 0 && name === "tracks" && this.model.cellClick && this.look) {
        const at = list.columnAt(x);
        if (at) this.model.cellClick(i, at.col.id, at.dx, this.look.textW("\u2605"));
      }
      if (i >= 0 && name === "tracks") {
        this.dragArm = { kind: "rows", rows: list.selection() };
        this.canvas.draggable = true;
      }
      this.invalidate();
    }
  }

  /** a list's selection changed: tell the page @param {string} name */
  listChanged(name) {
    if (name === "tracks") this.model.selectionChanged(this.tracks.selection());
    else this.model.filterChanged(this.artists.selection(), this.albums.selection());
  }

  /** @param {PointerEvent} e @param {any} drag */
  grab(e, drag) {
    this.drag = drag;
    try {
      this.canvas.setPointerCapture(e.pointerId);
    } catch {}
    e.preventDefault();
  }

  /** one step of a held scrollbar part @param {string} owner @param {ScrollBar} bar @param {string} part @param {number} [x] @param {number} [y] */
  step(owner, bar, part, x, y) {
    if ((part === "pageup" || part === "pagedown") && x !== undefined && y !== undefined && this.look) {
      // a held page stops once the thumb reaches the mouse
      const now = bar.hit(x, y, this.look.g);
      if (now !== part) return;
    }
    const line = bar.vertical ? 1 : 16;
    const page = bar.vertical ? Math.max(1, bar.page - 0) : Math.max(1, bar.page);
    const d = { up: -line, down: line, pageup: -page, pagedown: page }[part] ?? 0;
    if (owner === "tree") {
      this.tree.top = Math.max(0, Math.min(this.tree.top + d, this.tree.items.length - this.tree.bar.page));
      this.tree.bar.pos = this.tree.top;
    } else bar.set(bar.pos + d);
    this.invalidate();
  }

  /** @param {PointerEvent} e */
  move(e) {
    const [x, y] = this.at(e);
    const d = this.drag;
    if (!d) {
      this.hover(x, y);
      return;
    }
    if (d.kind === "ldiv") {
      this.ldiv = Math.round(x - d.off);
      this.invalidate();
    } else if (d.kind === "div1") {
      const vw = /** @type {any} */ (this.view).w || 1;
      const pos = Math.round(x - d.off) - (/** @type {any} */ (this.view).x + 1);
      this.filterPos = Math.max(0, Math.min(100000, Math.floor((pos * 100000) / vw)));
      this.invalidate();
    } else if (d.kind === "hdelim") {
      const pos = Math.round(y - d.off);
      this.mediaPos = Math.max(0, Math.min(100000, Math.floor((pos * 100000) / Math.max(1, this.h - 2))));
      this.invalidate();
    } else if (d.kind === "bar" && d.bar.held === "thumb" && this.look) {
      const bar = /** @type {ScrollBar} */ (d.bar);
      const at = (bar.vertical ? y - bar.rect.y : x - bar.rect.x) - d.bar.grab;
      const [, size, work] = bar.thumb(this.look.g);
      const tp = Math.max(0, Math.min(work - size, at - bar.arrow));
      bar.trackPos = tp;
      const span = bar.max + 1 - Math.max(1, bar.page);
      const pos = work - size > 0 ? Math.round((tp * span) / (work - size)) : 0;
      if (d.owner === "tree") {
        this.tree.top = Math.max(0, Math.min(pos, this.tree.items.length - this.tree.bar.page));
        bar.pos = this.tree.top;
      } else bar.set(pos);
      this.invalidate();
    } else if (d.kind === "column") {
      d.col.w = Math.max(0, Math.round(d.w0 + x - d.start));
      this.invalidate();
    } else if (d.kind === "button") {
      const b = /** @type {Button} */ (d.button);
      const over = b.hit(x, y) === d.part;
      if (b.pushed !== over) {
        b.pushed = over;
        this.invalidate();
      }
    } else if (d.kind === "header") {
      const h = d.list.hit(x, y);
      const pressed = h && "header" in h && h.header === d.index ? d.index : -1;
      if (d.list.pressedHeader !== pressed) {
        d.list.pressedHeader = pressed;
        this.invalidate();
      }
    }
  }

  /** the cursor for what's under it @param {number} x @param {number} y */
  hover(x, y) {
    let cursor = "";
    const vd = this.lines.vdelim;
    const d1 = this.lines.div1;
    const hd = this.lines.hdelim;
    if ((vd && inside(vd, x, y)) || (d1 && inside(d1, x, y))) cursor = "ew-resize";
    else if (hd && inside({ ...hd, w: hd.w + 2 }, x, y)) cursor = "ns-resize";
    else {
      const l = this.listAt(x, y);
      const h = l?.[1].hit(x, y);
      if (h && "edge" in h) cursor = "col-resize";
    }
    if (this.canvas.style.cursor !== cursor) this.canvas.style.cursor = cursor;
  }

  /** @param {PointerEvent} e */
  up(e) {
    const d = this.drag;
    const [x, y] = this.at(e);
    this.drag = null;
    clearTimeout(this.heldTimer);
    clearInterval(this.heldTimer);
    if (this.dragArm?.click !== undefined) {
      // pressed on the selection and let go without dragging: that row alone
      const i = this.dragArm.click;
      this.tracks.clickRow(i, false, false);
      this.listChanged("tracks");
      // (and a click on its stars rates it, as on any row)
      const at = this.model.cellClick && this.look ? this.tracks.columnAt(x) : null;
      if (at && this.look) this.model.cellClick?.(i, at.col.id, at.dx, this.look.textW("★"));
      this.invalidate();
    }
    this.dragArm = null;
    this.canvas.draggable = false;
    if (!d) return;
    if (d.kind === "bar") {
      d.bar.held = null;
      d.bar.trackPos = -1;
      this.invalidate();
    } else if (d.kind === "button") {
      const b = /** @type {Button} */ (d.button);
      const was = b.pushed;
      b.pushed = false;
      this.invalidate();
      if (was && b.hit(x, y) === d.part) this.press(b, d.part, e);
    } else if (d.kind === "header") {
      d.list.pressedHeader = -1;
      const h = d.list.hit(x, y);
      if (h && "header" in h && h.header === d.index) this.model.sort(d.name, d.list.columns[d.index].id);
      this.invalidate();
    } else if (d.kind === "column") {
      this.model.columnsChanged(d.name, Object.fromEntries(d.list.columns.map((/** @type {Column} */ c) => [c.id, c.w])));
    } else if (d.kind === "ldiv" || d.kind === "div1" || d.kind === "hdelim") {
      this.model.layoutChanged({ ldiv: this.ldiv, media: this.mediaPos, filter: this.filterPos });
    }
  }

  release() {
    if (this.drag) this.drag = null;
    clearTimeout(this.heldTimer);
    clearInterval(this.heldTimer);
  }

  /** a button pressed @param {Button} b @param {string} part @param {MouseEvent} e */
  press(b, part, e) {
    // (where a button's menu goes: by the button, as Winamp put it)
    this.pressedRect = { ...b.rect };
    if (b === this.libBtn) this.model.library(e);
    else if (b === this.clearBtn) {
      this.input.value = "";
      this.syncEdit(false);
      this.model.clear();
    } else if (b === this.artBtn) this.model.toolbar("art", e);
    else if (b === this.viewBtn) this.model.toolbar("view", e);
    else if (b === this.colsBtn) this.model.toolbar("columns", e);
    else this.model.button(/** @type {any} */ (b).id, /** @type {"main" | "split"} */ (part === "split" ? "split" : "main"), e);
  }

  /** @param {MouseEvent} e */
  dbl(e) {
    const [x, y] = this.at(e);
    if (this.divX > 0 && inside(this.tree.rect, x, y)) {
      const hit = this.tree.itemAt(x, y);
      if (hit && !hit.onArrow) this.model.treeActivate(hit.item.key);
      return;
    }
    const l = this.listAt(x, y);
    if (l && l[0] === "tracks") {
      const h = l[1].hit(x, y);
      if (h && "row" in h && h.row !== undefined && h.row >= 0) this.model.rowActivate(h.row);
    }
  }

  /** @param {MouseEvent} e */
  context(e) {
    e.preventDefault();
    const [x, y] = this.at(e);
    if (this.divX > 0 && inside(this.tree.rect, x, y)) {
      const hit = this.tree.itemAt(x, y);
      if (!hit) return;
      this.setFocus("tree");
      this.model.treeContext(hit.item.key, e);
      return;
    }
    const l = this.listAt(x, y);
    if (l && l[0] === "tracks") {
      this.setFocus("tracks");
      const h = l[1].hit(x, y);
      if (!h || !("row" in h) || h.row === undefined || h.row < 0) return;
      if (!l[1].selected.has(h.row)) {
        l[1].clickRow(h.row, false, false);
        this.listChanged("tracks");
        this.invalidate();
      }
      this.model.rowContext(h.row, e);
    }
  }

  /** @param {WheelEvent} e */
  wheel(e) {
    e.preventDefault();
    const [x, y] = this.at(e);
    const notches = Math.sign(e.deltaY) * Math.max(1, Math.round(Math.abs(e.deltaY) / 100));
    if (this.divX > 0 && inside(this.treeBox ?? this.tree.rect, x, y)) {
      this.tree.top = Math.max(0, Math.min(this.tree.top + notches * 3, this.tree.items.length - this.tree.bar.page));
      this.tree.bar.pos = this.tree.top;
      this.invalidate();
      return;
    }
    const l = this.listAt(x, y);
    if (!l) return;
    const list = l[1];
    if (e.shiftKey || (Math.abs(e.deltaX) > Math.abs(e.deltaY) && e.deltaX)) list.hbar.set(list.hbar.pos + Math.sign(e.deltaX || e.deltaY) * 48);
    else list.vbar.set(list.vbar.pos + notches * 3);
    this.invalidate();
  }

  /** @param {DragEvent} e */
  dragStart(e) {
    const arm = this.dragArm;
    this.dragArm = null;
    if (!arm) return e.preventDefault();
    const ok = arm.kind === "rows" ? this.model.rowsDrag(arm.rows, e) : this.model.treeDrag?.(arm.key, e);
    if (!ok) e.preventDefault();
  }

  /** @param {KeyboardEvent} e */
  key(e) {
    if (this.focus === "tree") return this.treeKey(e);
    const name = this.focus === "artists" || this.focus === "albums" ? this.focus : "tracks";
    const list = this[name];
    const rows = Math.max(1, list.rows);
    let to = -1;
    const cur = list.focus >= 0 ? list.focus : -1;
    switch (e.key) {
      case "ArrowDown":
        to = Math.min(list.count - 1, cur + 1);
        break;
      case "ArrowUp":
        to = Math.max(0, cur - 1);
        break;
      case "PageDown":
        to = Math.min(list.count - 1, cur + rows - 1);
        break;
      case "PageUp":
        to = Math.max(0, cur - rows + 1);
        break;
      case "Home":
        to = 0;
        break;
      case "End":
        to = list.count - 1;
        break;
      case "Enter":
        // (Shift+Enter: Enqueue selection, as Winamp's menu says)
        if (name === "tracks" && e.shiftKey) this.model.button("enqueue", "main", new MouseEvent("click"));
        else if (name === "tracks" && cur >= 0) this.model.rowActivate(cur);
        e.preventDefault();
        return;
      case "f":
      case "F":
        // Ctrl+F: Explore item folder
        if (e.ctrlKey && name === "tracks") {
          this.model.button("explore", "main", new MouseEvent("click"));
          e.preventDefault();
        }
        return;
      case "a":
      case "A":
        if (e.ctrlKey) {
          for (let i = 0; i < list.count; i++) list.selected.add(i);
          this.listChanged(name);
          this.invalidate();
          e.preventDefault();
        }
        return;
      default:
        return;
    }
    e.preventDefault();
    if (to < 0 || list.count === 0) return;
    if (e.shiftKey) {
      if (list.anchor < 0) list.anchor = Math.max(0, cur);
      list.clickRow(to, e.ctrlKey, true);
    } else if (e.ctrlKey) list.focus = to;
    else list.clickRow(to, false, false);
    list.show(to);
    this.listChanged(name);
    this.invalidate();
  }

  /** @param {KeyboardEvent} e */
  treeKey(e) {
    const items = this.tree.items;
    const i = items.findIndex((it) => it.key === this.tree.selected);
    const item = items[i];
    let to = -1;
    switch (e.key) {
      case "ArrowDown":
        to = Math.min(items.length - 1, i + 1);
        break;
      case "ArrowUp":
        to = Math.max(0, i - 1);
        break;
      case "Home":
        to = 0;
        break;
      case "End":
        to = items.length - 1;
        break;
      case "ArrowRight":
        if (item?.parent && !item.open) this.model.treeToggle(item.key);
        else if (item?.parent) to = i + 1;
        break;
      case "ArrowLeft":
        if (item?.parent && item.open) this.model.treeToggle(item.key);
        else if (item && item.level > 0) {
          for (let k = i - 1; k >= 0; k--)
            if (items[k].level < item.level) {
              to = k;
              break;
            }
        }
        break;
      case "Enter":
        if (item) this.model.treeActivate(item.key);
        break;
      default:
        return;
    }
    e.preventDefault();
    if (to >= 0 && items[to]) {
      this.tree.selected = items[to].key;
      this.tree.show(to);
      this.model.treeSelect(items[to].key);
      this.invalidate();
    }
  }

  destroy() {
    cancelAnimationFrame(this.frame);
    clearInterval(this.caretTimer);
    clearTimeout(this.heldTimer);
    clearInterval(this.heldTimer);
  }
}

/** outside the app (a test page): the same ops drawn by the browser */
function drawFallback(/** @type {CanvasRenderingContext2D} */ ctx, /** @type {MlOp[]} */ ops, /** @type {Look} */ look) {
  for (const op of ops) {
    ctx.save();
    if (op.k === "f") {
      ctx.fillStyle = `rgb(${op.c.join(",")})`;
      ctx.fillRect(op.x, op.y, op.w, op.h);
    } else if (op.k === "i") {
      const bin = atob(op.d);
      const data = new Uint8ClampedArray(bin.length);
      for (let i = 0; i < bin.length; i++) data[i] = bin.charCodeAt(i);
      const c = canvasOf(op.w, op.h);
      c.getContext("2d")?.putImageData(new ImageData(data, op.w, op.h), 0, 0);
      ctx.drawImage(c, op.x, op.y);
    } else if (op.k === "t") {
      if (op.clip) {
        ctx.beginPath();
        ctx.rect(op.clip[0], op.clip[1], op.clip[2], op.clip[3]);
        ctx.clip();
      }
      ctx.fillStyle = `rgb(${op.c.join(",")})`;
      ctx.font = `${look.font.size}px ${look.font.face}`;
      ctx.textBaseline = "middle";
      const f = op.f ?? 0;
      ctx.textAlign = f & DT.RIGHT ? "right" : f & DT.CENTER ? "center" : "left";
      const tx = f & DT.RIGHT ? op.x + op.w : f & DT.CENTER ? op.x + op.w / 2 : op.x;
      ctx.fillText(op.t, tx, f & DT.VCENTER ? op.y + op.h / 2 : op.y + look.fontH / 2);
    }
    ctx.restore();
  }
}

/** the toolbar buttons' little pictures (Spotiamp+'s own), a menu's arrow beside two of them @param {"art" | "view" | "cols"} kind */
function toolbarIcon(kind) {
  const c = canvasOf(kind === "art" ? 13 : 20, 11);
  const ctx = /** @type {CanvasRenderingContext2D} */ (c.getContext("2d"));
  ctx.fillStyle = "#fff";
  ctx.strokeStyle = "#fff";
  ctx.lineWidth = 1;
  if (kind !== "art") {
    ctx.fillRect(15, 4, 5, 1);
    ctx.fillRect(16, 5, 3, 1);
    ctx.fillRect(17, 6, 1, 1);
  }
  if (kind === "art") {
    ctx.strokeRect(0.5, 0.5, 12, 10);
    ctx.beginPath();
    ctx.moveTo(1, 10);
    ctx.lineTo(5, 5);
    ctx.lineTo(8, 8);
    ctx.lineTo(10, 6);
    ctx.lineTo(12, 9);
    ctx.lineTo(12, 10);
    ctx.fill();
    ctx.fillRect(3, 2, 2, 2);
  } else if (kind === "view") {
    ctx.strokeRect(0.5, 0.5, 12, 10);
    ctx.fillRect(1, 5, 11, 1);
    ctx.fillRect(6, 1, 1, 4);
  } else {
    ctx.strokeRect(0.5, 0.5, 12, 10);
    ctx.fillRect(4, 1, 1, 9);
    ctx.fillRect(8, 1, 1, 9);
  }
  return c;
}

/**
 * The look the controls draw with: the sheet and the skin's font, its
 * height from Windows. Texts are measured by Windows as they come up; until
 * a text's size is back it's guessed, and `onUpdate` repaints once it is.
 * @param {import("./genex.js").Genex} genex @param {import("./native.js").MlFont} font @param {() => void} onUpdate
 */
export async function createLook(genex, font, onUpdate) {
  const [fontH] = await metrics(font);
  /** @type {Map<string, number[]>} */
  const sizes = new Map();
  /** @type {Set<string>} */
  const pending = new Set();
  let queued = false;
  const flush = async () => {
    queued = false;
    const texts = [...pending];
    pending.clear();
    if (!texts.length) return;
    const got = await measure(font, texts);
    texts.forEach((t, i) => sizes.set(t, got[i]));
    if (sizes.size > 5000) sizes.clear();
    onUpdate();
  };
  /** @param {string[]} texts */
  const measureSync = (texts) =>
    texts.map((t) => {
      const known = sizes.get(t);
      if (known) return known;
      pending.add(t);
      if (!queued) {
        queued = true;
        setTimeout(flush, 0);
      }
      return [Math.round(t.length * font.size * 0.52), fontH];
    });
  return new Look(genex, font, fontH, measureSync);
}

export { Look };
