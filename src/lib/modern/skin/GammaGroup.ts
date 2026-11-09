// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { clamp, colorRgb, cssColor, normalizeDomId, num } from "../utils";
import Bitmap from "./Bitmap";
// import { glTransformImage } from "./GammaWebGL";

// https://www.pawelporwisz.pl/winamp/wct_en.php
export default class GammaGroup {
  _id: string;
  _value: string = "0,0,0";
  _boost: number = 0;
  _gray: number = 0;
  // Spotiamp+: the value's three numbers, as Winamp reads them (0 for none)
  _r: number = 0;
  _g: number = 0;
  _b: number = 0;

  // Spotiamp+: what a picture or colour of a group is filtered with, as
  // Winamp's colour themes give it: the group's values plus the set's
  // "General" ones, each within +-4095 (General's alone for no group)
  static combine(own: GammaGroup | null, general: GammaGroup | null): GammaGroup {
    const lim = (v: number) => Math.min(4095, Math.max(-4095, v));
    const g = new GammaGroup();
    g._id = own?._id ?? general?._id ?? "dummy";
    g._r = lim((own?._r ?? 0) + (general?._r ?? 0));
    g._g = lim((own?._g ?? 0) + (general?._g ?? 0));
    g._b = lim((own?._b ?? 0) + (general?._b ?? 0));
    g._gray = (own?._gray ?? 0) | (general?._gray ?? 0);
    g._boost = own?._boost || general?._boost ? 1 : 0;
    g._value = `${g._r},${g._g},${g._b}`;
    return g;
  }

  /** Spotiamp+: nothing to do: Winamp leaves the picture or colour as it is */
  isIdentity(): boolean {
    return !this._r && !this._g && !this._b && !this._gray && !this._boost;
  }

  key(): string {
    return `${this._r},${this._g},${this._b}|${this._gray}|${this._boost}`;
  }

  setXmlAttributes(attributes: { [attrName: string]: string }) {
    for (const [key, value] of Object.entries(attributes)) {
      this.setXmlAttr(key, value);
    }
  }

  setXmlAttr(_key: string, value: string) {
    const key = _key.toLowerCase();
    switch (key) {
      case "id":
        this._id = value;
        break;
      case "value": {
        // (Spotiamp+: as Winamp: each number read up to what isn't one, 0
        // for a missing one)
        this._value = value;
        const [r, g, b] = String(value).split(",");
        this._r = parseInt(r, 10) || 0;
        this._g = parseInt(g, 10) || 0;
        this._b = parseInt(b, 10) || 0;
        break;
      }
      case "boost":
        this._boost = parseInt(value, 10) || 0;
        break;
      case "gray":
        this._gray = parseInt(value, 10) || 0;
        break;
      default:
        return false;
    }
    return true;
  }

  getId() {
    return this._id;
  }

  getDomId() {
    return normalizeDomId(this._id);
  }

  getRgb() {
    return cssColor(this._value);
  }

  transformImage(
    img: HTMLImageElement,
    x: number,
    y: number,
    w: number,
    h: number
  ): string {
    return this.transformCanvas(img, x, y, w, h).toDataURL();
  }

  /** Spotiamp+: the picture (its x, y, w, h part) in these colours, as a canvas */
  transformCanvas(
    img: CanvasImageSource,
    x: number,
    y: number,
    w: number,
    h: number
  ): HTMLCanvasElement {
    // because some <bitmap> didn't has explicit "w" attribute
    const safeWidth = w || img.width;
    const safeHeight = h || img.height;
    // because some <bitmap> didn't has explicit "x" attribute
    // if it is any, we threat it as background-position coordinate
    const safeLeft = x ? -x : 0;
    const safeTop = y ? -y : 0;
    const canvas = document.createElement("canvas");
    canvas.width = safeWidth;
    canvas.height = safeHeight;
    // (Spotiamp+: read back right away: kept on the CPU)
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/drawImage
    ctx.drawImage(img, safeLeft, safeTop);
    const imageData = ctx.getImageData(0, 0, safeWidth, safeHeight);
    const data = imageData.data;
    // Spotiamp+: as Winamp's gamma filter: on the picture premultiplied, as
    // its PNG loader keeps it; grey, then boost (half the colour plus half
    // the alpha), then each channel times (65535 + 16 v) / 65536
    const rm = 65535 + this._r * 16;
    const gm = 65535 + this._g * 16;
    const bm = 65535 + this._b * 16;
    const gray = this._gray;
    const boost = !!this._boost;
    for (let i = 0; i < data.length; i += 4) {
      const a = data[i + 3];
      let ir = data[i];
      let ig = data[i + 1];
      let ib = data[i + 2];
      if (a !== 255) {
        ir = (ir * a) >> 8;
        ig = (ig * a) >> 8;
        ib = (ib * a) >> 8;
      }
      if (gray === 1) ir = ig = ib = Math.max(ir, ig, ib);
      else if (gray === 2) ir = ig = ib = Math.floor((ir + ig + ib) / 3);
      if (boost) {
        const half = a >> 1;
        ir = (ir >> 1) + half;
        ig = (ig >> 1) + half;
        ib = (ib >> 1) + half;
      }
      ir = Math.min(255, Math.floor((ir * rm) / 65536));
      ig = Math.min(255, Math.floor((ig * gm) / 65536));
      ib = Math.min(255, Math.floor((ib * bm) / 65536));
      if (a !== 255) {
        // back to the canvas's straight colours; a channel brighter than
        // the alpha (Winamp adds it over what's below: a glass's glow) gets
        // the alpha raised to it, exact over black and close over the rest
        const aa = Math.max(a, ir, ig, ib);
        ir = aa ? Math.min(255, Math.round((ir * 255) / aa)) : 0;
        ig = aa ? Math.min(255, Math.round((ig * 255) / aa)) : 0;
        ib = aa ? Math.min(255, Math.round((ib * 255) / aa)) : 0;
        data[i + 3] = aa;
      }
      data[i] = ir;
      data[i + 1] = ig;
      data[i + 2] = ib;
    }
    ctx.putImageData(imageData, 0, 0);
    return canvas;
  }

  transformColor2rgb(color: string): [number, number, number] {
    // (Spotiamp+: "#rrggbb" too; white for no colour, never NaN)
    let [red, green, blue] = (colorRgb(color) ?? [255, 255, 255]).map((v) => Math.round(v));
    if (this.isIdentity()) return [red, green, blue];
    // Spotiamp+: as Winamp's gamma filter for a colour: grey, boost (half the
    // colour plus 128), then each channel times (65536 + 16 v) / 65536
    if (this._gray === 1) red = green = blue = Math.max(red, green, blue);
    else if (this._gray === 2) red = green = blue = Math.floor((red + green + blue) / 3);
    if (this._boost) {
      red = (red >> 1) + 128;
      green = (green >> 1) + 128;
      blue = (blue >> 1) + 128;
    }
    red = clamp(Math.floor((red * (65536 + this._r * 16)) / 65536), 0, 255);
    green = clamp(Math.floor((green * (65536 + this._g * 16)) / 65536), 0, 255);
    blue = clamp(Math.floor((blue * (65536 + this._b * 16)) / 65536), 0, 255);
    return [red, green, blue];
  }

  transformColor(color: string): string {
    const [red, green, blue] = this.transformColor2rgb(color);
    return `rgb(${red}, ${green}, ${blue})`;
  }
}
