// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import MakiMap from "./MakiMap";

/**
 * Spotiamp+: a Winamp region, a shape a skin cuts from a map (a grey
 * picture: what's past the threshold is inside) or a bitmap (what isn't
 * see-through), moved by offset; put on a part (GuiObj.setRegion), the part
 * shows only inside it.
 */
export default class Region {
  static GUID = "3a370c02439f3cbf8886f184361ecf5b";
  _uiRoot: any;
  /** the shape: opaque where it's inside */
  _mask: HTMLCanvasElement | null = null;
  _x: number = 0;
  _y: number = 0;

  constructor(uiRoot?: any) {
    this._uiRoot = uiRoot;
  }

  loadfrommap(regionmap: MakiMap, threshold: number, reversed: boolean) {
    this._mask = maskFromMap(regionmap, threshold, reversed);
    this._x = this._y = 0;
  }

  loadfrombitmap(bitmapId: string) {
    const canvas = this._uiRoot?.getBitmap(bitmapId)?.getCanvas?.();
    this._mask = canvas ?? null;
    this._x = this._y = 0;
  }

  offset(x: number, y: number) {
    this._x += Number(x) || 0;
    this._y += Number(y) || 0;
  }

  /** On `div`: it shows only inside the shape (none: all of it again). */
  _applyTo(div: HTMLElement) {
    if (!this._mask || this._mask.width < 1 || this._mask.height < 1) {
      div.style.removeProperty("mask-image");
      return;
    }
    div.style.maskImage = `url(${this._mask.toDataURL()})`;
    div.style.maskRepeat = "no-repeat";
    div.style.maskPosition = `${this._x}px ${this._y}px`;
    div.style.maskSize = "auto";
  }
}

/** A map's shape: opaque where its grey is past the threshold (below it, reversed). */
export function maskFromMap(map: MakiMap, threshold: number, reversed: boolean): HTMLCanvasElement | null {
  const source = map?._bitmap?.getCanvas?.(true);
  if (!source || source.width < 1 || source.height < 1) return null;
  const { width, height } = source;
  const data = source.getContext("2d").getImageData(0, 0, width, height).data;
  const mask = document.createElement("canvas");
  mask.width = width;
  mask.height = height;
  const ctx = mask.getContext("2d", { willReadFrequently: true });
  const out = ctx.createImageData(width, height);
  const limit = Number(threshold) || 0;
  for (let i = 0; i < data.length; i += 4) {
    const inside = data[i] > limit !== Boolean(reversed);
    out.data[i + 3] = inside ? 255 : 0;
  }
  ctx.putImageData(out, 0, 0);
  return mask;
}
