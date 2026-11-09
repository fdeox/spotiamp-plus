// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import Group from "./Group";
import { lengthText, paintPe, rowName, winampPeAvailable } from "../winampPe";
import { bitmapCanvas, drawWasabiButton } from "../wasabiPaint";

/**
 * Spotiamp+: Winamp 5's playlist editor as a modern skin showed it. Winamp
 * held its classic playlist window in the skin's place, 10 px left of it,
 * 20 px above, 5 px right and 38 px below (gen_ff's region offsets for it),
 * so its title bar and buttons were out of sight and the list, its sides
 * and its scrollbar showed. Its picture (pledit.bmp) was made from the
 * skin's colours and buttons, and so were its colours (gen_ff's
 * doplaylistcolors), unless the skin brought its own pledit.bmp/pledit.txt.
 * The rows are painted the way Winamp painted them (winampPe.ts), and the
 * mouse, the wheel and the keys do what they did there (Peui.cpp,
 * Pledit.cpp). Read for its behaviour; none of its code is used.
 *
 * Coordinates below are the classic window's own, as Winamp's were: the list
 * from x 12 to width - 20 and y 20 to height - 38, its scrollbar at width - 15.
 */

// the region offsets: left, top, right, bottom
const OFF_L = 10;
const OFF_T = 20;
const OFF_R = 5;
const OFF_B = 38;
// Winamp's playlist font and size (custom_plfont on, its defaults)
const PE_FONT = "Arial";
const PE_FONTSIZE = 12;
const WHEEL_DELTA = 120;

// ---- colours as Winamp kept them: a COLORREF is 0x00BBGGRR, a bitmap's
// pixel 0xAARRGGBB; RGBTOBGR swaps the two ----
const rgbToBgr = (c: number) => (((c & 0xff) << 16) | (c & 0xff00) | ((c >>> 16) & 0xff)) >>> 0;
/** Blenders::BLEND_AVG: each byte halved and added */
const blendAvg = (a: number, b: number) => (((a >>> 1) & 0xff7f7f7f) + ((b >>> 1) & 0xff7f7f7f)) >>> 0;
/** gen_ff's lumidiff, whose MIN((r, g), b) kept only the last two bytes */
const lumidiff = (a: number, b: number) =>
  Math.min(Math.abs(((a >>> 8) & 0xff) - ((b >>> 8) & 0xff)), Math.abs((a & 0xff) - (b & 0xff)));
const maxByte = (c: number) => Math.max((c >>> 16) & 0xff, (c >>> 8) & 0xff, c & 0xff);
const colorrefRgb = (c: number): [number, number, number] => [c & 0xff, (c >>> 8) & 0xff, (c >>> 16) & 0xff];
const argbRgb = (c: number): [number, number, number] => [(c >>> 16) & 0xff, (c >>> 8) & 0xff, c & 0xff];
const css = ([r, g, b]: number[]) => `rgb(${r},${g},${b})`;

type Look = {
  /** normal text, the playing row's, background, selected background */
  colors: [number, number, number][];
  /** the playlist window's picture: Winamp's made one or the skin's pledit.bmp */
  plbm: HTMLCanvasElement;
};

export default class PlayListGui extends Group {
  static GUID = "45f3f7c14ee6a6f35e125ea18d3ffc92";
  /** the commands a status bar around it shows (gen_ff's PlaylistAppCmds) */
  _appCmds = [
    { name: "Add", right: false, menu: "pe_add" },
    { name: "Rem", right: false, menu: "pe_rem" },
    { name: "Sel", right: false, menu: "pe_sel" },
    { name: "Misc", right: false, menu: "pe_misc" },
    { name: "List", right: true, menu: "pe_list" },
  ];
  // static guid = "{45F3F7C1-A6F3-4EE6-A15E-125E92FC3F8D}";
  _canvas: HTMLCanvasElement = document.createElement("canvas");
  /** the first row shown (pledit_disp_offs) */
  _offs: number = 0;
  /** the font's height, and how many rows fit (num_songs) */
  _rowH: number = 15;
  _numSongs: number = 0;
  _sel: Set<number> = new Set();
  /** where a shift-click selects from (shiftsel_1) */
  _shiftsel: number = -1;
  _look: Look | null = null;
  /** the skin's own pledit.txt colours / pledit.bmp, when it has them */
  _skinColors: [number, number, number][] | null = null;
  _skinPlbm: HTMLCanvasElement | null = null;
  _capture: "" | "lb" | "vs" = "";
  _movePos: number = 0;
  _stt: boolean = false;
  _clickYoffs: number = 9;
  _carry: number = 0;
  _last: { x: number; y: number } = { x: 0, y: 0 };
  _edgeTimer: any = null;
  _painting: boolean = false;
  _again: boolean = false;
  _resize: ResizeObserver | null = null;
  _disposers: Function[] = [];

  getElTag(): string {
    return "group";
  }

  init() {
    super.init();
    const pl = this._uiRoot.playlist;
    this._disposers.push(
      pl.on("trackchange", this._rowsChanged),
      pl.on("showtrack", (i: number) => this._ensureVisible(i)),
      this._uiRoot.on("colorthemechanged", () => {
        this._look = null;
        this._schedule();
      })
    );
    this._readSkinFiles().finally(() => {
      this._look = null;
      this._schedule();
    });
    this._schedule();
  }

  dispose() {
    for (const d of this._disposers) {
      try {
        d?.();
      } catch {}
    }
    this._disposers = [];
    this._resize?.disconnect();
    clearInterval(this._edgeTimer);
    super.dispose?.();
  }

  draw() {
    super.draw();
    const div = this._div;
    div.classList.add("pl");
    div.setAttribute("tabindex", "0");
    div.style.pointerEvents = "auto";
    div.style.outline = "none";
    this._canvas.classList.add("pe");
    div.appendChild(this._canvas);
    this._canvas.addEventListener("pointerdown", this._onDown);
    this._canvas.addEventListener("pointermove", this._onMove);
    this._canvas.addEventListener("pointerup", this._onUp);
    this._canvas.addEventListener("pointercancel", this._onUp);
    this._canvas.addEventListener("dblclick", this._onDblClick);
    this._canvas.addEventListener("wheel", this._onWheel, { passive: false });
    this._canvas.addEventListener("contextmenu", this._onContext);
    div.addEventListener("keydown", this._onKey);
    this._resize = new ResizeObserver(() => this._schedule());
    this._resize.observe(div);
  }

  // ---- the playlist ----

  _count(): number {
    return this._uiRoot.playlist.getnumtracks();
  }

  _rowsChanged = () => {
    const n = this._count();
    for (const i of [...this._sel]) if (i >= n) this._sel.delete(i);
    this._syncSelection(false);
    this._schedule();
  };

  _isSel(i: number): boolean {
    return this._sel.has(i);
  }

  _setSel(i: number, on: boolean) {
    if (i < 0 || i >= this._count()) return;
    if (on) this._sel.add(i);
    else this._sel.delete(i);
  }

  _clearSel() {
    this._sel.clear();
  }

  /** the selection, for the skin's scripts and for Spotiamp+'s menus */
  _syncSelection(tell: boolean = true) {
    const list = [...this._sel].sort((a, b) => a - b);
    this._uiRoot.playlist._selection = list;
    if (tell) this._uiRoot.playlist.commands.select?.(list);
  }

  // ---- sizes, as Winamp had them ----

  _size() {
    const w = Math.round(this._div?.offsetWidth ?? 0);
    const h = Math.round(this._div?.offsetHeight ?? 0);
    // the classic window held there
    return { w, h, W: w + OFF_L + OFF_R, H: h + OFF_T + OFF_B };
  }

  _clampOffs() {
    const t = this._count() - this._numSongs;
    if (this._offs > t) this._offs = t;
    if (this._offs < 0) this._offs = 0;
  }

  _ensureVisible(i: number) {
    if (i == null || i < 0) return;
    if (i < this._offs) this._offs = i;
    else if (this._numSongs > 0 && i >= this._offs + this._numSongs) this._offs = i - this._numSongs + 1;
    this._clampOffs();
    this._schedule();
  }

  // ---- the look ----

  async _readSkinFiles() {
    // a skin's own pledit.txt / pledit.bmp, in its folder (Winamp looked
    // there, as for a classic skin)
    const ex = this._uiRoot._fileExtractor;
    const own = (name: string) => ex?._files?.get(((ex._root ?? "") + name).toLowerCase()) ?? null;
    try {
      const txt = own("pledit.txt");
      if (txt) {
        const bytes = await txt.async("uint8array");
        this._skinColors = parsePleditTxt(new TextDecoder("latin1").decode(bytes));
      }
    } catch {}
    try {
      const bmp = own("pledit.bmp");
      if (bmp) {
        const blob = await bmp.async("blob");
        const image = await createImageBitmap(blob);
        const c = document.createElement("canvas");
        c.width = Math.max(280, image.width);
        c.height = Math.max(186, image.height);
        c.getContext("2d").drawImage(image, 0, 0);
        this._skinPlbm = c;
      }
    } catch {}
  }

  _hasColor(id: string): boolean {
    const lower = id.toLowerCase();
    return this._uiRoot._colors.some((c) => String(c._id).toLowerCase() === lower);
  }

  /** SkinColor: the colour in the colour theme on, as a COLORREF; magenta
   *  when the skin hasn't it */
  _skinColor(id: string): number {
    if (!this._hasColor(id)) return 0xffff00ff;
    const rgb = this._uiRoot._namedColorRgb(id);
    if (!rgb) return 0xffff00ff;
    return (0xff000000 | rgb[0] | (rgb[1] << 8) | (rgb[2] << 16)) >>> 0;
  }

  /** a skin bitmap in the colour theme on, as a canvas */
  _bitmapCanvas(id: string): HTMLCanvasElement | null {
    return bitmapCanvas(this._uiRoot, id);
  }

  /** skin_getBitmapColor: the bitmap drawn on a canvas one wider and higher,
   *  the pixel Wasabi read there (its row stride one short) */
  _bitmapColor(id: string): number {
    const c = this._bitmapCanvas(id);
    // (no such bitmap: Wasabi read past an empty canvas, black)
    if (!c || !c.width || !c.height) return 0;
    const w = c.width;
    const h = c.height;
    const big = document.createElement("canvas");
    big.width = w + 1;
    big.height = h + 1;
    const ctx = big.getContext("2d", { willReadFrequently: true });
    ctx.drawImage(c, 0, 0);
    const i = (w >> 1) + (h >> 1) * w + 1;
    const [r, g, b, a] = ctx.getImageData(i % (w + 1), Math.floor(i / (w + 1)), 1, 1).data;
    // (premultiplied, as Wasabi's canvas kept it)
    const pm = (v: number) => Math.round((v * a) / 255);
    return ((a << 24) | (pm(r) << 16) | (pm(g) << 8) | pm(b)) >>> 0;
  }

  /** getWindowBackground: the skin's wasabi.window.background, else the
   *  colour of a window of it, else of its base texture (a bitmap's pixel) */
  _windowBackground(): number {
    if (this._hasColor("wasabi.window.background")) return rgbToBgr(this._skinColor("wasabi.window.background"));
    const pixel = (id: string, centre: boolean) => {
      const c = this._bitmapCanvas(id);
      if (!c || !c.width || !c.height) return 0;
      const ctx = c.getContext("2d", { willReadFrequently: true });
      const [r, g, b] = ctx.getImageData(centre ? c.width >> 1 : 0, centre ? c.height >> 1 : 0, 1, 1).data;
      return ((r << 16) | (g << 8) | b) >>> 0;
    };
    // (Winamp drew a window of the skin and read the middle of it: its
    // frame's texture)
    return pixel("wasabi.frame.basetexture", true) || pixel("wasabi.basetexture", false) || 0x646464;
  }

  _makeLook(): Look {
    const interpolate = (this._uiRoot._skinVersion ?? 1) * 10 < 10;
    const wb = this._windowBackground(); // a bitmap pixel
    const has = (id: string) => this._hasColor(id);
    const sc = (id: string) => this._skinColor(id);
    // gen_ff's doplaylistcolors
    const buf = [0, 0, 0, 0];
    buf[2] = (has("wasabi.list.background") ? sc("wasabi.list.background") : rgbToBgr(this._bitmapColor("wasabi.list.background"))) & 0xffffff;
    buf[0] = (has("wasabi.list.text") ? sc("wasabi.list.text") : sc("wasabi.edit.text")) & 0xffffff;
    if (interpolate) {
      let c = buf[0];
      c = lumidiff(c, buf[2]) < 0x1f ? blendAvg(buf[0], 0xff7f7f7f) : c;
      c = lumidiff(c, buf[2]) < 0x1f ? blendAvg(buf[0], 0xff101010) : c;
      buf[0] = c & 0xffffff;
    }
    let selected: number;
    if (has("wasabi.list.text.selected.background")) {
      selected = sc("wasabi.list.text.selected.background");
      buf[3] = selected;
    } else {
      selected = has("wasabi.list.item.selected") ? rgbToBgr(sc("wasabi.list.item.selected")) : rgbToBgr(sc("wasabi.tree.selected"));
      let col = rgbToBgr(sc("wasabi.list.column.background"));
      col = maxByte(col) < 0x1f ? blendAvg(wb, 0xff000000) : col;
      const listbkg = rgbToBgr(buf[2]);
      const steps: [number, number][] = [
        [wb, 0xff7f7f7f],
        [listbkg, 0xff7f7f7f],
        [wb, 0xfff0f0f0],
        [listbkg, 0xfff0f0f0],
        [col, 0xfff0f0f0],
        [col, 0xff101010],
      ];
      for (const [a, b] of steps) selected = lumidiff(selected, listbkg) < 0x2f ? blendAvg(a, b) : selected;
      const text = rgbToBgr(buf[0]);
      selected = lumidiff(selected, text) < 0x1f ? blendAvg(selected, 0xff101010) : selected;
      selected = lumidiff(selected, text) < 0x1f ? blendAvg(selected, 0xfff0f0f0) : selected;
      selected = (selected & 0xffffff) >>> 0;
      buf[3] = rgbToBgr(selected);
    }
    if (has("wasabi.list.text.current")) {
      buf[1] = sc("wasabi.list.text.current");
    } else {
      const far = (act: number) => lumidiff(act, buf[0]) < 0x3f || lumidiff(act, buf[2]) < 0x2f;
      let act = blendAvg(selected, buf[0]);
      act = far(act) ? blendAvg(buf[2], 0xffffffff) : act;
      act = far(act) ? blendAvg(buf[2], 0xff101010) : act;
      act = far(act) ? blendAvg(selected, buf[0]) : act;
      act = far(act) ? blendAvg(selected, buf[2]) : act;
      act = far(act) ? blendAvg(buf[0], buf[2]) : act;
      buf[1] = act & 0xffffff;
    }
    let colors = buf.map((c) => colorrefRgb(c >>> 0)) as [number, number, number][];
    if (this._skinColors) colors = this._skinColors;

    let plbm = this._skinPlbm;
    if (!plbm) {
      // Winamp's own pledit.bmp for the skin: its window background, the
      // scrollbar's button (and pressed) and its track
      plbm = document.createElement("canvas");
      plbm.width = 280;
      plbm.height = 186;
      const ctx = plbm.getContext("2d");
      ctx.imageSmoothingEnabled = false;
      ctx.fillStyle = css(argbRgb(wb));
      ctx.fillRect(0, 0, 280, 186);
      drawWasabiButton(this._uiRoot, ctx, 52, 53, 8, 18, false, "wasabi.scrollbar.vertical.grip");
      drawWasabiButton(this._uiRoot, ctx, 61, 53, 8, 18, true, "wasabi.scrollbar.vertical.grip");
      let inner: number;
      if (has("wasabi.scrollbar.background.inverted")) inner = rgbToBgr(sc("wasabi.scrollbar.background.inverted"));
      else if (has("wasabi.list.column.empty")) inner = rgbToBgr(sc("wasabi.list.column.empty"));
      else {
        let col = sc("wasabi.list.column.background");
        col = maxByte(col) < 0x1f ? blendAvg(wb, 0xff000000) : col;
        inner = rgbToBgr(blendAvg(col, 0xff000000));
      }
      const edge = has("wasabi.border.sunken") ? rgbToBgr(sc("wasabi.border.sunken")) : blendAvg(wb, maxByte(wb) > 0xe0 ? 0xff000000 : 0xffffffff);
      ctx.fillStyle = css(argbRgb(inner));
      ctx.fillRect(36, 42, 8, 29);
      ctx.fillStyle = css(argbRgb(edge));
      ctx.fillRect(36, 42, 1, 29);
      ctx.fillRect(43, 42, 1, 29);
    }
    return { colors, plbm };
  }

  // ---- painting ----

  _schedule = () => {
    if (this._painting) {
      this._again = true;
      return;
    }
    this._painting = true;
    Promise.resolve()
      .then(() => this._paint())
      .catch((e) => console.warn("playlist editor:", e))
      .finally(() => {
        this._painting = false;
        if (this._again) {
          this._again = false;
          this._schedule();
        }
      });
  };

  async _paint() {
    const { w, h, W, H } = this._size();
    if (w < 1 || h < 1) return;
    if (!this._look) this._look = this._makeLook();
    const look = this._look;
    const pl = this._uiRoot.playlist;
    const count = this._count();
    const current = pl.getcurrentindex();
    const listW = W - 32;
    const listH = H - 58;
    this._numSongs = Math.max(0, Math.floor((H - 38 - 20 - 2) / this._rowH));
    this._clampOffs();
    // the rows from the first shown, a few more than fit
    const rowsFor = (n: number) => {
      const rows = [];
      for (let i = this._offs; i < count && rows.length < n; i++) {
        const sec = pl._tracks[i]?.duration;
        rows.push({ name: rowName(i, count, pl.gettitle(i)), time: sec > 0 ? sec : -1, sel: (this._isSel(i) ? 1 : 0) | (i === current ? 2 : 0) });
      }
      return rows;
    };
    let rowsImage: HTMLCanvasElement | null = null;
    if (listW > 0 && listH > 0 && winampPeAvailable()) {
      const ask = () => paintPe({ face: PE_FONT, size: PE_FONTSIZE, w: listW, h: listH, rows: rowsFor(this._numSongs + 2), colors: look.colors });
      let painted = await ask();
      if (painted && painted.rowH !== this._rowH) {
        // the font's own height (Windows'): again with the rows that fit
        this._rowH = painted.rowH;
        this._numSongs = painted.rows;
        this._clampOffs();
        painted = await ask();
      }
      if (painted) {
        this._numSongs = painted.rows;
        rowsImage = document.createElement("canvas");
        rowsImage.width = painted.w;
        rowsImage.height = painted.h;
        rowsImage.getContext("2d").putImageData(new ImageData(painted.data, painted.w, painted.h), 0, 0);
      }
    }
    if (!rowsImage && listW > 0 && listH > 0) rowsImage = this._paintRowsHere(listW, listH, rowsFor(this._numSongs + 2), look);

    // the whole place, made off screen, then shown at once
    const out = document.createElement("canvas");
    out.width = w;
    out.height = h;
    const ctx = out.getContext("2d");
    ctx.imageSmoothingEnabled = false;
    ctx.translate(-OFF_L, -OFF_T);
    const blit = (dx: number, dy: number, bw: number, bh: number, sx: number, sy: number, sw = bw, sh = bh) => {
      if (bw > 0 && bh > 0 && sw > 0 && sh > 0) ctx.drawImage(look.plbm, sx, sy, sw, sh, dx, dy, bw, bh);
    };
    // draw_pl: its sides, 29 px at a time, what's left squeezed in
    let yp = 20;
    for (let n = Math.floor((H - 20 - 38) / 29); n > 0; n--) {
      blit(0, yp, 12, 29, 0, 42);
      blit(W - 20, yp, 5, 29, 31, 42);
      blit(W - 7, yp, 7, 29, 44, 42);
      yp += 29;
    }
    const rest = (H - 20 - 38) % 29;
    if (rest > 0) {
      blit(0, yp, 12, rest, 0, 42, 12, 29);
      blit(W - 20, yp, 5, rest, 31, 42, 5, 29);
      blit(W - 7, yp, 7, rest, 44, 42, 7, 29);
    }
    if (rowsImage) ctx.drawImage(rowsImage, 12, 20);
    this._paintSlider(blit, W, H, this._capture === "vs");
    const show = this._canvas;
    if (show.width !== w) show.width = w;
    if (show.height !== h) show.height = h;
    show.style.width = `${w}px`;
    show.style.height = `${h}px`;
    show.getContext("2d").drawImage(out, 0, 0);
  }

  /** draw_pe_vslide: the track, 29 px at a time, and the button on it */
  _paintSlider(blit, W: number, H: number, pushed: boolean) {
    const trackH = H - 20 - 38;
    const slidH = 18;
    let yp = 0;
    const t = this._count() - this._numSongs;
    if (t >= 1) yp = Math.max(0, Math.trunc(((trackH - slidH) * Math.min(this._offs, t)) / t));
    const x = W - 15;
    let y = 0;
    for (; y < yp - 28; y += 29) blit(x, 20 + y, 8, 29, 36, 42);
    let oy = y + 29;
    if (slidH + yp > oy) oy += 29;
    if (y < yp) blit(x, 20 + y, 8, yp - y, 36, 42);
    blit(x, 20 + yp, 8, slidH, pushed ? 61 : 52, 53);
    y = yp + slidH;
    if (y < oy && y < trackH) blit(x, 20 + y, 8, y < trackH - 29 ? oy - y : trackH - y, 36, 42 + 29 - (oy - y));
    for (y = oy; y < trackH; y += 29) blit(x, 20 + y, 8, y < trackH - 29 ? 29 : trackH - y, 36, 42);
  }

  /** the rows without Windows' help (a page outside the app): close to it */
  _paintRowsHere(w: number, h: number, rows, look: Look): HTMLCanvasElement {
    const c = document.createElement("canvas");
    c.width = w;
    c.height = h;
    const ctx = c.getContext("2d");
    const [fg, cur, bg, selbg] = look.colors;
    ctx.fillStyle = css(bg);
    ctx.fillRect(0, 0, w, h);
    ctx.font = `${PE_FONTSIZE}px ${PE_FONT}, sans-serif`;
    ctx.textBaseline = "top";
    for (let pos = 0; pos < this._numSongs && pos < rows.length; pos++) {
      const row = rows[pos];
      const y = 2 + pos * this._rowH;
      ctx.fillStyle = css(row.sel & 1 ? selbg : bg);
      ctx.fillRect(0, y, w, this._rowH);
      ctx.fillStyle = css(row.sel & 2 ? cur : fg);
      let right = w;
      if (row.time >= 0) {
        const text = lengthText(row.time);
        const tw = Math.ceil(ctx.measureText(text).width);
        ctx.fillText(text, w - tw, y + 1);
        right -= tw;
      }
      let name = row.name;
      if (ctx.measureText(name).width > right) {
        while (name.length && ctx.measureText(name + "...").width > right) name = name.slice(0, -1);
        name += "...";
      }
      ctx.fillText(name, 0, y + 1);
    }
    return c;
  }

  // ---- the mouse (Peui.cpp) ----

  /** a pointer event's spot in the classic window's coordinates */
  _point(e: MouseEvent) {
    const r = this._canvas.getBoundingClientRect();
    const { w, h } = this._size();
    const sx = r.width ? w / r.width : 1;
    const sy = r.height ? h / r.height : 1;
    return { x: Math.floor((e.clientX - r.left) * sx) + OFF_L, y: Math.floor((e.clientY - r.top) * sy) + OFF_T };
  }

  _rowAt(y: number): number {
    return Math.trunc((y - 22) / this._rowH) + this._offs;
  }

  _onDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    this._div.focus({ preventScroll: true });
    const { x, y } = this._point(e);
    const { W, H } = this._size();
    this._last = { x, y };
    if (x >= W - 15 && x < W - 7 && y >= 20 && y < H - 38) {
      try {
        this._canvas.setPointerCapture(e.pointerId);
      } catch {}
      this._capture = "vs";
      this._vscroll(y, true);
      e.preventDefault();
    } else if (x >= 12 && x < W - 20 && y >= 20 && y < H - 38) {
      try {
        this._canvas.setPointerCapture(e.pointerId);
      } catch {}
      this._listDown(y, e.ctrlKey, e.shiftKey);
      e.preventDefault();
    }
  };

  _onMove = (e: PointerEvent) => {
    if (!this._capture) return;
    const { x, y } = this._point(e);
    this._last = { x, y };
    if (this._capture === "vs") this._vscroll(y, false);
    else this._listMove(y);
  };

  _onUp = (e: PointerEvent) => {
    if (!this._capture) return;
    const { y } = this._point(e);
    try {
      this._canvas.releasePointerCapture(e.pointerId);
    } catch {}
    clearInterval(this._edgeTimer);
    this._edgeTimer = null;
    if (this._capture === "vs") {
      this._capture = "";
      this._schedule();
      return;
    }
    this._capture = "";
    if (!this._stt) {
      // a press on a selected row: it alone, once let go
      const wh = this._rowAt(y);
      if (!e.ctrlKey && this._isSel(wh)) {
        this._clearSel();
        this._setSel(wh, true);
        this._syncSelection();
        this._schedule();
      }
    }
    this._stt = false;
  };

  /** do_vscroll: the button follows the mouse, where it was taken hold of */
  _vscroll(y: number, first: boolean) {
    const { H } = this._size();
    const top = 20;
    const span = H - 38 - 20 - 18;
    const a = this._count() - this._numSongs;
    const p = a < 1 || span <= 0 ? top : top + Math.trunc((span * this._offs) / a);
    if (first) this._clickYoffs = y >= p && y < p + 20 ? y - p - 1 : 9;
    if (span > 0) {
      let d = Math.trunc(((y - this._clickYoffs - top) * a) / span);
      if (d > a) d = a;
      this._offs = d;
      this._clampOffs();
    }
    this._schedule();
  }

  /** do_lb, the press: what's selected, and where a drag starts */
  _listDown(y: number, ctrl: boolean, shift: boolean) {
    const count = this._count();
    const wh = this._rowAt(y);
    if (!ctrl && !this._isSel(wh)) this._clearSel();
    if (shift) {
      if (this._shiftsel !== -1) {
        const t = Math.max(Math.min(count, this._shiftsel), Math.min(count, wh));
        if (!ctrl && this._isSel(wh)) this._clearSel();
        for (let i = Math.min(this._shiftsel, wh); i <= t; i++) this._setSel(i, true);
        this._stt = true;
      }
    } else {
      if (this._isSel(wh) && ctrl) this._setSel(wh, false);
      else {
        const pos = wh - this._offs;
        if (pos < count - this._offs && pos < this._numSongs) this._setSel(wh, true);
      }
      this._shiftsel = wh;
    }
    this._movePos = this._rowAt(y);
    this._capture = "lb";
    this._syncSelection();
    this._schedule();
  }

  /** do_lb, the drag: the selected rows move along with the mouse; above
   *  or below the list it scrolls */
  _listMove(y: number) {
    const { H } = this._size();
    const m = this._rowAt(y);
    // held above or below the list: it goes on, as Winamp's timer went
    const outside = y < 10 || y > H - 28;
    if (outside && !this._edgeTimer) {
      this._edgeTimer = setInterval(() => {
        if (this._capture === "lb") this._listMove(this._last.y);
      }, 50);
    } else if (!outside && this._edgeTimer) {
      clearInterval(this._edgeTimer);
      this._edgeTimer = null;
    }
    if (m === this._movePos) return;
    const pl = this._uiRoot.playlist;
    const v = this._count();
    const swaps: [number, number][] = [];
    while (m > this._movePos) {
      if (!this._isSel(v - 1)) {
        for (let x = v - 2; x >= 0; x--) {
          if (!this._isSel(x)) continue;
          if (this._shiftsel === x) this._shiftsel = x + 1;
          this._swap(x, x + 1);
          swaps.push([x, x + 1]);
        }
      }
      this._movePos++;
    }
    while (m < this._movePos) {
      if (!this._isSel(0)) {
        for (let x = 1; x < v; x++) {
          if (!this._isSel(x)) continue;
          if (this._shiftsel === x) this._shiftsel = x - 1;
          this._swap(x, x - 1);
          swaps.push([x, x - 1]);
        }
      }
      this._movePos--;
    }
    for (const [from, to] of swaps) pl.commands.move(from, to);
    if (y < 10) {
      this._offs--;
      if (this._offs < 0) {
        this._offs = 0;
        this._movePos = 0;
      }
    } else if (y > H - 28) {
      const t = v - this._numSongs;
      this._offs++;
      if (this._offs > t) {
        this._offs = Math.max(t, 0);
        this._movePos = v;
      }
    }
    if (swaps.length) this._syncSelection();
    this._schedule();
  }

  /** PlayList_swap here at once (Spotiamp+'s playlist is told too): the
   *  rows, their selection and which one plays */
  _swap(a: number, b: number) {
    const pl = this._uiRoot.playlist;
    const rows = pl._tracks;
    if (!rows[a] || !rows[b]) return;
    [rows[a], rows[b]] = [rows[b], rows[a]];
    const sa = this._isSel(a);
    const sb = this._isSel(b);
    this._setSel(a, sb);
    this._setSel(b, sa);
    if (pl._currentIndex === a) pl._currentIndex = b;
    else if (pl._currentIndex === b) pl._currentIndex = a;
  }

  _onDblClick = (e: MouseEvent) => {
    const { x, y } = this._point(e);
    const { W, H } = this._size();
    if (x >= 12 && y >= 20 && x <= W - 20 && y <= H - 38) {
      const t = Math.trunc((y - 22) / this._rowH);
      if (t < this._count() - this._offs && t < this._numSongs) this._uiRoot.playlist.playtrack(t + this._offs);
    }
  };

  _onWheel = (e: WheelEvent) => {
    e.preventDefault();
    // a wheel notch is 120 to Windows
    let zDelta = Math.round(e.deltaMode === 1 ? -e.deltaY * 40 : e.deltaMode === 2 ? -e.deltaY * 360 : (-e.deltaY / 100) * WHEEL_DELTA);
    if ((zDelta < 0 && this._carry > 0) || (zDelta > 0 && this._carry < 0)) this._carry = 0;
    else zDelta += this._carry;
    // (twice as far, Winamp's default, unless the middle button is down:
    // then the selected rows move)
    const middle = (e.buttons & 4) !== 0;
    if (!middle) zDelta *= 2;
    let dLines = Math.trunc(zDelta / WHEEL_DELTA);
    this._carry = zDelta - dLines * WHEEL_DELTA;
    if (!dLines) {
      if (zDelta > 0) dLines = 1;
      else if (zDelta < 0) dLines = 0;
    }
    for (let n = Math.abs(dLines); n > 0; n--) {
      if (middle) this._moveSelected(dLines >= 0 ? -1 : 1);
      else this._scroll(dLines >= 0 ? -1 : 1);
    }
  };

  /** ID_PE_SCROLLUP / DOWN: a row (config_plscrollsize) */
  _scroll(by: number) {
    const t = this._count() - this._numSongs;
    this._offs += by;
    if (by > 0 && this._offs >= t) this._offs = t;
    if (this._offs < 0) this._offs = 0;
    this._schedule();
  }

  _onContext = (e: MouseEvent) => {
    // a row not selected becomes the one selected, then Spotiamp+'s menu
    // (the window's own right-click) shows for it
    const { x, y } = this._point(e);
    const { W, H } = this._size();
    if (x >= 12 && x < W - 20 && y >= 20 && y < H - 38) {
      const wh = this._rowAt(y);
      if (wh < this._count() && !this._isSel(wh)) {
        this._clearSel();
        this._setSel(wh, true);
        this._shiftsel = wh;
        this._syncSelection();
        this._schedule();
      }
    }
  };

  // ---- the keys (Pledit.cpp) ----

  _onKey = (e: KeyboardEvent) => {
    const count = this._count();
    const key = e.key;
    let done = true;
    if ((key === "ArrowUp" || key === "ArrowDown") && e.altKey) this._moveSelected(key === "ArrowUp" ? -1 : 1);
    else if (key === "ArrowUp" || key === "ArrowDown") this._step(key === "ArrowUp", e.shiftKey);
    else if (key === "PageUp" || key === "PageDown") {
      for (let n = Math.max(1, this._numSongs - 1); n > 0; n--) this._step(key === "PageUp", e.shiftKey, false);
      this._syncSelection();
    } else if (key === "Home" || key === "End") {
      if (!count) return;
      const which = key === "Home" ? 0 : count - 1;
      if (!e.shiftKey) this._clearSel();
      this._setSel(which, true);
      this._shiftsel = which;
      this._ensureVisible(which);
      this._syncSelection();
    } else if (key === "Enter") {
      const first = [...this._sel].sort((a, b) => a - b)[0];
      if (first != null) this._uiRoot.playlist.playtrack(first);
    } else if (key === "Delete") {
      const list = [...this._sel].sort((a, b) => b - a);
      this._clearSel();
      this._syncSelection();
      for (const i of list) this._uiRoot.playlist.commands.remove(i);
    } else if ((key === "a" || key === "A") && e.ctrlKey && !e.altKey) {
      for (let i = 0; i < count; i++) this._sel.add(i);
      this._syncSelection();
    } else done = false;
    if (done) {
      e.preventDefault();
      e.stopPropagation();
      this._schedule();
    }
  };

  /** ID_PE_SCUP / SCDOWN: the row above the first selected one shown (or
   *  below the last), alone unless shift is down, and into view */
  _step(up: boolean, shift: boolean, tell: boolean = true) {
    const count = this._count();
    if (!count) return;
    const n = this._numSongs;
    let which: number;
    let x: number;
    if (up) {
      for (x = this._offs; x < this._offs + n; x++) if (this._isSel(x)) break;
      which = x !== this._offs + n ? x - 1 : this._offs;
    } else {
      for (x = this._offs + n - 1; x >= this._offs; x--) if (this._isSel(x)) break;
      which = x >= this._offs ? x + 1 : this._offs + n - 1;
    }
    if (which < 0) which = 0;
    if (which >= count) which = count - 1;
    if (!shift) this._clearSel();
    this._setSel(which, true);
    this._shiftsel = which;
    if (which < this._offs) this._offs = which;
    else if (which >= this._offs + n) this._offs = which - n + 1;
    this._clampOffs();
    if (tell) this._syncSelection();
  }

  /** ID_PE_MOVEUP / MOVEDOWN: the selected rows a row up or down */
  _moveSelected(by: number) {
    const v = this._count();
    const pl = this._uiRoot.playlist;
    const swaps: [number, number][] = [];
    if (by < 0) {
      if (this._isSel(0)) return;
      for (let x = 1; x < v; x++) {
        if (!this._isSel(x)) continue;
        this._swap(x, x - 1);
        swaps.push([x, x - 1]);
      }
    } else {
      if (this._isSel(v - 1)) return;
      for (let x = v - 2; x >= 0; x--) {
        if (!this._isSel(x)) continue;
        this._swap(x, x + 1);
        swaps.push([x, x + 1]);
      }
    }
    for (const [from, to] of swaps) pl.commands.move(from, to);
    this._syncSelection();
    this._schedule();
  }
}

/** A classic skin's pledit.txt colours: [Text] Normal, Current, NormalBG,
 *  SelectedBG ("#rrggbb"), Winamp's own where it hasn't one. */
export function parsePleditTxt(text: string): [number, number, number][] {
  const out: [number, number, number][] = [
    [0, 255, 0],
    [255, 255, 255],
    [0, 0, 0],
    [0, 0, 198],
  ];
  const keys = ["normal", "current", "normalbg", "selectedbg"];
  let inText = false;
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    const section = line.match(/^\[(.*)\]$/);
    if (section) {
      inText = section[1].trim().toLowerCase() === "text";
      continue;
    }
    if (!inText) continue;
    const m = line.match(/^([^=]+)=\s*#?([0-9a-f]{6})/i);
    if (!m) continue;
    const at = keys.indexOf(m[1].trim().toLowerCase());
    if (at < 0) continue;
    const v = parseInt(m[2], 16);
    out[at] = [(v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff];
  }
  return out;
}
