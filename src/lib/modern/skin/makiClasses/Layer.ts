// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import GuiObj from "./GuiObj";
import Movable from "./Movable";
import Region from "./Region";

// http://wiki.winamp.com/wiki/XML_GUI_Objects#.3Clayer.2F.3E
export default class Layer extends Movable {
  // Spotiamp+: Winamp's Layer.setRegionFromMap: the layer shows only where
  // the map is past the threshold
  setregionfrommap(regionmap: any, threshold: number, reversed: boolean) {
    const region = new Region(this._uiRoot);
    region.loadfrommap(regionmap, threshold, reversed);
    region._applyTo(this._div);
  }

  static GUID = "5ab9fa1545579a7d5765c8aba97cc6a6";
  _image: string;
  _inactiveImage: string;
  _tile: boolean = false;

  setXmlAttr(key: string, value: string): boolean {
    if (super.setXmlAttr(key, value)) {
      if (key == "sysregion") {
        this._renderRegion();
      }
      return true;
    }
    switch (key) {
      case "image":
        this._image = value;
        this._renderBackground();
        this._renderRegion();
        if (this._fx.enabled) this._fxRender();
        break;
      case "inactiveimage":
        this._inactiveImage = value;
        this._renderBackground();
        break;
      case "tile":
        this._tile = value !== "0";
        this._renderBackground();
        break;
      default:
        return false;
    }
    return true;
  }

  // This shadows `getheight()` on GuiObj
  getheight(): number {
    const rel = this._relativeSize("h");
    if (rel != null) return rel;
    if (this._h) {
      return this._h;
    }
    if (this._image != null) {
      const bitmap = this._uiRoot.getBitmap(this._image);
      if (bitmap) return bitmap.getHeight();
    }
    return super.getheight();
  }

  // This shadows `getwidth()` on GuiObj
  getwidth(): number {
    const rel = this._relativeSize("w");
    if (rel != null) return rel;
    if (this._w) {
      return this._w;
    }
    if (this._image != null) {
      const bitmap = this._uiRoot.getBitmap(this._image);
      if (bitmap) return bitmap.getWidth();
    }
    return super.getwidth();
  }

  // (Spotiamp+: stretched to the layer, or tiled, as Wasabi draws it; a
  // kind of layer that places its picture itself keeps its own way)
  _stretches(): boolean {
    return this.constructor === Layer;
  }

  _renderBackground() {
    const bitmap =
      this._image != null ? this._uiRoot.getBitmap(this._image) : null;
    this.setBackgroundImage(bitmap);
    this.setInactiveBackgroundImage(bitmap);
    if (this._stretches()) {
      this._div.style.backgroundSize = this._tile ? "auto" : "100% 100%";
      this._div.style.backgroundRepeat = this._tile ? "repeat" : "no-repeat";
    }
    if (this._inactiveImage) {
      this.setInactiveBackgroundImage(
        this._uiRoot.getBitmap(this._inactiveImage)
      );
      this._div.classList.add("inactivable");
    }
  }

  // (Spotiamp+: as Wasabi: sysregion="1" makes the layer's opaque part part
  // of the window's shape (the app's region); it never cuts the layer's own
  // picture, whose see-through pixels are see-through anyway. Webamp cut it
  // along a traced outline, which a concave picture got wrong: Classic
  // Modern's equalizer lost its right side)
  _renderRegion() {
    this._div.style.removeProperty("clip-path");
  }

  draw() {
    super.draw();
    this._div.classList.add("webamp--img");
    this._renderBackground();
  }

  dispose() {
    this._fxStop();
    super.dispose();
  }

  // Spotiamp+: Winamp's layer effects (fx_*), like AVS's Dynamic Movement:
  // the layer's picture drawn through a grid whose every point the skin's
  // script moves (fx_onGetPixelR/D turn it round the middle and push it
  // out, X/Y place it, A makes it see-through), the pixels in between
  // filled in from the points around them. Mostly what turns a skin's
  // knobs, dials and discs.
  _fx = {
    enabled: false,
    wrap: false,
    rect: false,
    bgfx: false,
    clear: false,
    speed: 25,
    realtime: false,
    localized: false,
    bilinear: true,
    alpha: false,
    gw: 16,
    gh: 16,
  };
  _fxCanvas: HTMLCanvasElement | null = null;
  _fxTimer: any = null;
  _fxBusy: boolean = false;
  _fxAgain: boolean = false;
  _fxSource: { canvas: HTMLCanvasElement; data: Uint8ClampedArray; w: number; h: number } | null = null;

  fx_setenabled(on: boolean) {
    const was = this._fx.enabled;
    this._fx.enabled = Boolean(on);
    if (this._fx.enabled && !was) this._fxStart();
    else if (!this._fx.enabled && was) this._fxStop();
  }
  fx_getenabled(): boolean {
    return this._fx.enabled;
  }
  fx_setwrap(on: boolean) {
    this._fx.wrap = Boolean(on);
  }
  fx_getwrap(): boolean {
    return this._fx.wrap;
  }
  fx_setrect(on: boolean) {
    this._fx.rect = Boolean(on);
  }
  fx_getrect(): boolean {
    return this._fx.rect;
  }
  fx_setbgfx(on: boolean) {
    this._fx.bgfx = Boolean(on);
  }
  fx_getbgfx(): boolean {
    return this._fx.bgfx;
  }
  fx_setclear(on: boolean) {
    this._fx.clear = Boolean(on);
  }
  fx_getclear(): boolean {
    return this._fx.clear;
  }
  fx_setspeed(msperframe: number) {
    this._fx.speed = Math.max(16, Number(msperframe) || 0);
    this._fxTiming();
  }
  fx_getspeed(): number {
    return this._fx.speed;
  }
  fx_setrealtime(on: boolean) {
    this._fx.realtime = Boolean(on);
    this._fxTiming();
  }
  fx_getrealtime(): boolean {
    return this._fx.realtime;
  }
  fx_setlocalized(on: boolean) {
    this._fx.localized = Boolean(on);
  }
  fx_getlocalized(): boolean {
    return this._fx.localized;
  }
  fx_setbilinear(on: boolean) {
    this._fx.bilinear = Boolean(on);
  }
  fx_getbilinear(): boolean {
    return this._fx.bilinear;
  }
  fx_setalphamode(on: boolean) {
    this._fx.alpha = Boolean(on);
  }
  fx_getalphamode(): boolean {
    return this._fx.alpha;
  }
  fx_setgridsize(x: number, y: number) {
    const cells = (n: number) => Math.min(64, Math.max(1, Math.round(Number(n) || 1)));
    this._fx.gw = cells(x);
    this._fx.gh = cells(y);
  }
  fx_update() {
    if (this._fx.enabled) this._fxRender();
  }
  fx_restart() {
    if (!this._fx.enabled) return;
    this._uiRoot.vm.dispatch(this, "fx_oninit", []).then(() => this._fxRender());
  }

  _fxStart() {
    if (!this._fxCanvas) {
      const canvas = document.createElement("canvas");
      canvas.style.cssText =
        "position:absolute;left:0;top:0;width:100%;height:100%;pointer-events:none";
      this._fxCanvas = canvas;
    }
    this._div.appendChild(this._fxCanvas);
    // the picture is drawn by the effect now, not as the layer's background
    this._div.style.setProperty("background-image", "none", "important");
    this._uiRoot.vm.dispatch(this, "fx_oninit", []).then(() => this._fxRender());
    this._fxTiming();
  }

  _fxStop() {
    clearInterval(this._fxTimer);
    this._fxTimer = null;
    this._fxCanvas?.remove();
    this._div?.style.removeProperty("background-image");
  }

  /** realtime: a new frame every `speed` ms, the script told first (fx_onFrame) */
  _fxTiming() {
    clearInterval(this._fxTimer);
    this._fxTimer = null;
    if (!this._fx.enabled || !this._fx.realtime) return;
    this._fxTimer = setInterval(async () => {
      if (this._fxBusy || !this._div.isConnected) return;
      await this._uiRoot.vm.dispatch(this, "fx_onframe", []);
      this._fxRender();
    }, this._fx.speed);
  }

  /** draws now, or right after the frame being drawn */
  async _fxRender() {
    if (this._fxBusy) {
      this._fxAgain = true;
      return;
    }
    this._fxBusy = true;
    try {
      do {
        this._fxAgain = false;
        await this._fxDraw();
      } while (this._fxAgain && this._fx.enabled);
    } catch (e) {
      console.warn("layer effect", this._id, e);
    } finally {
      this._fxBusy = false;
    }
  }

  /** the layer's picture as pixels, read once per picture */
  _fxPixels() {
    const bitmap = this._image != null ? this._uiRoot.getBitmap(this._image) : null;
    if (!bitmap?.getImg?.()) return null;
    const canvas = bitmap.getCanvas(true);
    if (!canvas || canvas.width < 1 || canvas.height < 1) return null;
    if (this._fxSource?.canvas !== canvas) {
      const { width: w, height: h } = canvas;
      const data = canvas.getContext("2d").getImageData(0, 0, w, h).data;
      this._fxSource = { canvas, data, w, h };
    }
    return this._fxSource;
  }

  async _fxDraw() {
    const canvas = this._fxCanvas;
    const src = this._fxPixels();
    const w = Math.round(this.getwidth());
    const h = Math.round(this.getheight());
    if (!canvas || !src || w < 1 || h < 1) return;
    const { gw, gh, rect, alpha, wrap, clear, bilinear } = this._fx;
    const vm = this._uiRoot.vm;
    const has = (e: string) => vm.listens(this, e);
    const hasR = has("fx_ongetpixelr");
    const hasD = has("fx_ongetpixeld");
    const hasX = has("fx_ongetpixelx");
    const hasY = has("fx_ongetpixely");
    const hasA = alpha && has("fx_ongetpixela");

    // the grid: where each point takes its pixels from, and how see-through
    const n = (gw + 1) * (gh + 1);
    const us = new Float32Array(n);
    const vs = new Float32Array(n);
    const as = new Float32Array(n).fill(1);
    const cx = w / 2;
    const cy = h / 2;
    const maxd = Math.hypot(w, h) / 2;
    for (let j = 0, k = 0; j <= gh; j++) {
      for (let i = 0; i <= gw; i++, k++) {
        const xd = (i / gw) * w - cx;
        const yd = (j / gh) * h - cy;
        const x = xd / cx;
        const y = yd / cy;
        const d = Math.hypot(xd, yd) / maxd;
        const r = Math.atan2(yd, xd) + Math.PI / 2;
        const args = [r, d, x, y].map((value) => ({ type: "DOUBLE", value }));
        const ask = async (on: boolean, e: string, fallback: number) => {
          if (!on) return fallback;
          const v = Number(await vm.dispatchValue(this, e, args));
          return Number.isFinite(v) ? v : fallback;
        };
        if (rect) {
          us[k] = ((await ask(hasX, "fx_ongetpixelx", x)) + 1) * cx;
          vs[k] = ((await ask(hasY, "fx_ongetpixely", y)) + 1) * cy;
        } else {
          const turn = (await ask(hasR, "fx_ongetpixelr", r)) - Math.PI / 2;
          const dist = (await ask(hasD, "fx_ongetpixeld", d)) * maxd;
          us[k] = cx + Math.cos(turn) * dist;
          vs[k] = cy + Math.sin(turn) * dist;
        }
        if (hasA) as[k] = Math.min(1, Math.max(0, await ask(true, "fx_ongetpixela", 1)));
      }
    }
    if (!this._fx.enabled) return;

    // every pixel: from the grid points around it
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    const out = ctx.createImageData(w, h);
    const o = out.data;
    const s = src.data;
    const sw = src.w;
    const sh = src.h;
    const kx = sw / w;
    const ky = sh / h;
    const row = gw + 1;
    // a source pixel's place in `s`, -1 outside it (cleared)
    const at = (xi: number, yi: number) => {
      if (wrap) {
        xi = ((xi % sw) + sw) % sw;
        yi = ((yi % sh) + sh) % sh;
      } else if (xi < 0 || yi < 0 || xi >= sw || yi >= sh) {
        if (clear) return -1;
        xi = Math.min(sw - 1, Math.max(0, xi));
        yi = Math.min(sh - 1, Math.max(0, yi));
      }
      return (yi * sw + xi) * 4;
    };
    let p = 0;
    for (let py = 0; py < h; py++) {
      const gy = ((py + 0.5) / h) * gh;
      const j = Math.min(gh - 1, gy | 0);
      const fy = gy - j;
      for (let px = 0; px < w; px++, p += 4) {
        const gx = ((px + 0.5) / w) * gw;
        const i = Math.min(gw - 1, gx | 0);
        const fx = gx - i;
        const k00 = j * row + i;
        const k01 = k00 + row;
        const w00 = (1 - fx) * (1 - fy);
        const w10 = fx * (1 - fy);
        const w01 = (1 - fx) * fy;
        const w11 = fx * fy;
        const u = (us[k00] * w00 + us[k00 + 1] * w10 + us[k01] * w01 + us[k01 + 1] * w11) * kx - 0.5;
        const v = (vs[k00] * w00 + vs[k00 + 1] * w10 + vs[k01] * w01 + vs[k01 + 1] * w11) * ky - 0.5;
        const a = as[k00] * w00 + as[k00 + 1] * w10 + as[k01] * w01 + as[k01 + 1] * w11;
        if (bilinear) {
          const x0 = Math.floor(u);
          const y0 = Math.floor(v);
          const tx = u - x0;
          const ty = v - y0;
          let r = 0, g = 0, b = 0, al = 0;
          const add = (q: number, wt: number) => {
            if (q < 0 || wt <= 0) return;
            const qa = s[q + 3] * wt;
            r += s[q] * qa;
            g += s[q + 1] * qa;
            b += s[q + 2] * qa;
            al += qa;
          };
          add(at(x0, y0), (1 - tx) * (1 - ty));
          add(at(x0 + 1, y0), tx * (1 - ty));
          add(at(x0, y0 + 1), (1 - tx) * ty);
          add(at(x0 + 1, y0 + 1), tx * ty);
          if (al > 0) {
            o[p] = r / al;
            o[p + 1] = g / al;
            o[p + 2] = b / al;
            o[p + 3] = al * a;
          }
        } else {
          const q = at(Math.round(u), Math.round(v));
          if (q >= 0) {
            o[p] = s[q];
            o[p + 1] = s[q + 1];
            o[p + 2] = s[q + 2];
            o[p + 3] = s[q + 3] * a;
          }
        }
      }
    }
    ctx.putImageData(out, 0, 0);
  }
}
