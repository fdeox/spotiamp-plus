// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { UIRoot } from "../UIRoot";
import { assert, getId, hexToRgb, normalizeDomId, num, px } from "../utils";
import ImageManager from "./ImageManager";

export function genCssVar(bitmapId: string): string {
  return `--bitmap-${bitmapId.replace(/[^a-zA-Z0-9]/g, "-")}`;
}

// http://wiki.winamp.com/wiki/XML_Elements#.3Cbitmap.2F.3E
export default class Bitmap {
  _id: string;
  _uiRoot: UIRoot; // lazy set
  _cssVar: string;
  _url: string;
  _img: CanvasImageSource;
  _canvas: HTMLCanvasElement;
  _x: number = 0;
  _y: number = 0;
  _w: number;
  _h: number;
  _file: string;
  _transparentColor: string;
  _gammagroup: string;

  // I am not sure, is bitmap need UiRoot?
  constructor(uiRoot: UIRoot = null) {
    this._uiRoot = uiRoot;
  }
  setUiRoot(uiRoot: UIRoot) {
    this._uiRoot = uiRoot;
  }

  setXmlAttributes(attributes: { [attrName: string]: string }) {
    for (const [key, value] of Object.entries(attributes)) {
      this.setXmlAttr(key, value);
    }
  }

  setXmlAttr(_key: string, value: string): boolean {
    const key = _key.toLowerCase();
    switch (key) {
      case "id":
        this._id = value; //TODO: should be lowerCase here.
        this._cssVar = genCssVar(this.getId());
        break;
      case "x":
        this._x = num(value) ?? 0;
        break;
      case "y":
        this._y = num(value) ?? 0;
        break;
      case "w":
        this._w = num(value);
        break;
      case "h":
        this._h = num(value);
        break;
      case "file":
        this._file = value;
        break;
      case "gammagroup":
        this._gammagroup = value;
        break;
      case "transparentcolor":
        //seem as only windows media player uses it.
        this._transparentColor = value;
        break;
      default:
        return false;
    }
    return true;
  }

  getId() {
    return this._id || "";
  }

  getFile() {
    return this._file || "";
  }

  getWidth() {
    return this._w;
  }

  getHeight() {
    return this._h;
  }

  getLeft() {
    return this._x;
  }

  getTop() {
    return this._y;
  }

  getCSSVar(): string {
    return this._cssVar;
  }

  getGammaGroup(): string {
    return this._gammagroup;
  }

  getImg(): CanvasImageSource {
    return this._img;
  }
  setImage(img: CanvasImageSource) {
    // await imageManager.setImage(this._file, url);
    // await this.ensureImageLoaded(imageManager);
    this._img = img;
  }

  loaded(): boolean {
    return this._img != null;
  }

  // Ensure we've loaded the image into our image loader.
  async ensureImageLoaded(imageManager: ImageManager) {
    assert(
      this._url == null,
      "Tried to ensure a Bitmap was laoded more than once."
    );

    //force. also possibly set null:
    this._img = await imageManager.getImage(this._file);
    if (this._img && this._w == null && this._h == null) {
      this.setXmlAttr("w", String(this._img.width));
      this.setXmlAttr("h", String(this._img.height));
    }
  }

  _getBackgrondImageCSSAttribute(): string {
    return `var(${this.getCSSVar()})`;
  }

  _getBackgrondPositionCSSAttribute(): string {
    const x = px(-(this._x ?? 0));
    const y = px(-(this._y ?? 0));
    return `${x} ${y}`;
  }

  _getBackgrondSizeCSSAttribute(): string {
    const width = px(this._w);
    const height = px(this._h);
    return `${width} ${height}`;
  }

  _setAsBackground(div: HTMLElement, prefix: string) {
    div.style.setProperty(
      `--${prefix}background-image`,
      this._getBackgrondImageCSSAttribute()
    );
  }

  setAsBackground(div: HTMLElement) {
    this._setAsBackground(div, "");
  }

  setAsDownBackground(div: HTMLElement) {
    this._setAsBackground(div, "down-");
  }

  setAsActiveBackground(div: HTMLElement) {
    this._setAsBackground(div, "active-");
  }
  setAsInactiveBackground(div: HTMLElement) {
    this._setAsBackground(div, "inactive-");
  }

  setAsHoverBackground(div: HTMLElement) {
    this._setAsBackground(div, "hover-");
  }

  setAsHoverDownBackground(div: HTMLElement) {
    this._setAsBackground(div, "hover-down-");
  }

  setAsDisabledBackground(div: HTMLElement) {
    this._setAsBackground(div, "disabled-");
  }

  async getGammaTransformedUrl(uiRoot: UIRoot, sharp: boolean = true): Promise<string> {
    // (Spotiamp+: a picture of `k` times its pixels, said to be for a screen
    // of that many (image-set), shows at its own size)
    const buildCssProp = (url: string, k: number = 1) =>
      k > 1 ? `  ${this.getCSSVar()}: image-set(url(${url}) ${k}x);` : `  ${this.getCSSVar()}: url(${url});`;
    // Spotiamp+: the picture with each pixel made 2 x 2, shown at its own
    // size: on a screen at 125 % the skin's window smooths it (routes/modern)
    // from those, so its pixels come out evenly wide with soft edges only
    // where they meet, instead of every fourth one twice as wide (Winamp at
    // 125 % was Windows' blur). Not a big background (memory), nor where
    // its pixels are counted (border-image).
    const asCss = (c: HTMLCanvasElement): string => {
      if (!sharp || !c.width || !c.height || c.width * c.height > 1_000_000) return buildCssProp(c.toDataURL());
      const d = document.createElement("canvas");
      d.width = c.width * 2;
      d.height = c.height * 2;
      const dctx = d.getContext("2d");
      if (!dctx) return buildCssProp(c.toDataURL());
      dctx.imageSmoothingEnabled = false;
      dctx.drawImage(c, 0, 0, d.width, d.height);
      return buildCssProp(d.toDataURL(), 2);
    };
    const img = this.getImg();
    if (!img) {
      console.warn(`Bitmap/font ${this.getId()} has no img. skipped.`);
      return "";
    }

    const groupId = this.getGammaGroup();
    const gammaGroup = uiRoot._getGammaGroup(groupId);
    // (Spotiamp+: the picture in these colours made once: a skin switching
    // its colours, or switching to the same, doesn't redo all of them)
    const key = `${gammaGroup.key()}|${this.getCSSVar()}|${sharp}`;
    if (this._cssCache?.key === key) return this._cssCache.prop;
    let prop: string;
    if (gammaGroup.isIdentity()) {
      // all zero (boost too): Winamp leaves it as it is
      // (an animated GIF as it is: its frames aren't on a canvas)
      prop = this._file.endsWith(".gif") ? buildCssProp(await this.toDataURL(uiRoot)) : asCss(this.getCanvas());
    } else {
      prop = asCss(gammaGroup.transformCanvas(img as HTMLImageElement, this._x, this._y, this._w, this._h));
    }
    this._cssCache = { key, prop };
    return prop;
  }
  /**
   * Final function that uses Bitmap;
   * Afther call this, bitmap maybe destroyed.
   * @returns url string used as embedded in <head><style>
   */
  async toDataURL(uiRoot: UIRoot): Promise<string> {
    if (this._file.endsWith(".gif") /* && !this._transparentColor */) {
      // don't draw _img into canvas, if it is animated gif
      return await uiRoot.getImageManager().getUrl(this._file);
    } else {
      return this.getCanvas().toDataURL();
    }
  }

  /**
   * Useful for calculating clip-path
   * @param store whether the generated canvas should be
   *              kept by this bitmap instance
   * @returns <canvas/>
   */
  // Spotiamp+: the last coloured picture's CSS, and for which colours
  _cssCache: { key: string; prop: string } | null = null;

  getCanvas(store: boolean = false): HTMLCanvasElement {
    let workingCanvas: HTMLCanvasElement;
    if (this._canvas == null || !store) {
      assert(
        this._img != null,
        `Expected bitmap image to be loaded: ${this.getId()}`
      );
      if (this._img instanceof HTMLCanvasElement) {
        workingCanvas = this._img;
      } else {
        workingCanvas = document.createElement("canvas");
        workingCanvas.width = this.getWidth() /* || this._img.width */;
        workingCanvas.height = this.getHeight() /* || this._img.height */;
        // (Spotiamp+: read back often (colours, regions, PNG): kept on the CPU)
        const ctx = workingCanvas.getContext("2d", { willReadFrequently: true });
        // https://developer.mozilla.org/en-US/docs/Web/API/CanvasRenderingContext2D/drawImage
        ctx.drawImage(this._img, -this._x, -this._y);

        //set transparentColor if any
        if (/* applyTransparency &&  */ this._transparentColor != null) {
          const rgb = hexToRgb(this._transparentColor);
          // // get the image data object
          // var data = ctx.getImageData(0, 0, this._canvas.width, this._canvas.height).data;
          // get the image data object
          var image = ctx.getImageData(
            0,
            0,
            workingCanvas.width,
            workingCanvas.height
          );
          // get the image data values
          var data = image.data;
          const length = data.length;
          // set alpha=0 if match to color
          for (var i = 0; i < length; i += 4) {
            if (
              data[i + 0] == rgb.r &&
              data[i + 1] == rgb.g &&
              data[i + 2] == rgb.b
            ) {
              data[i + 3] = 0;
            }
          }
          // after the manipulation, reset the data
          // image.data = data;
          // and put the imagedata back to the canvas
          ctx.putImageData(image, 0, 0);
        }
      }

      if (store) {
        this._canvas = workingCanvas;
      }
    }
    if (store) {
      workingCanvas = this._canvas;
    }
    return workingCanvas;
  }
}
