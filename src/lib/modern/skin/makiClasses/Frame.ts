// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { num } from "../../utils";
import GuiObj from "./GuiObj";
import Group from "./Group";

// http://wiki.winamp.com/wiki/XML_GUI_Objects#.3CWasabi:Frame.2F.3E
//
// Spotiamp+: laid out as Winamp's own FrameWnd did: two groups (left and
// right, or top and bottom) with a divider between them, the divider's
// place counted in pixels from the frame's left/top edge, or from its
// right/bottom one; a negative maxwidth leaves at least that much to the
// other side, minwidth keeps the near side at least that wide; the divider
// is 8 pixels and only there while the frame can be resized. Laid out again
// whenever the frame's size changes (a skin's script making it taller),
// its groups told of their new size (onResize).

/** Winamp's PULLBAR_FULL, and the fractions of it a position may be. */
const PULLBAR_FULL = 65536;
const SIZER = 8;

export default class Frame extends Group {
  static GUID = "e2bbc14d417384f6ebb2b3bd5055662f";
  /** Where the divider is (Winamp's pullbarpos), from the `from` edge. */
  _position: number = 128;
  _orientation: string = "v";
  _resizable: boolean = true;
  _from: string = "left";
  _fromRight: boolean = false;
  _minwidth: number = 0;
  _maxwidth: number = 0;
  _leftId: string;
  _rightId: string;
  _topId: string;
  _bottomId: string;
  _vbitmap: string = "wasabi.framewnd.verticaldivider";
  _vgrabber: string = "wasabi.framewnd.verticalgrabber";
  _hbitmap: string = "wasabi.framewnd.horizontaldivider";
  _hgrabber: string = "wasabi.framewnd.horizontalgrabber";
  /** The left (or top) group and the right (or bottom) one (SkinEngine_WAL.frame). */
  _first: GuiObj | null = null;
  _second: GuiObj | null = null;
  _sizer: HTMLElement | null = null;
  _grabber: HTMLElement | null = null;
  _observer: ResizeObserver | null = null;
  _framed: boolean = false;

  getElTag(): string {
    return "frame2";
  }

  setXmlAttr(_key: string, value: string): boolean {
    const key = _key.toLowerCase();
    if (super.setXmlAttr(key, value)) {
      return true;
    }
    switch (key) {
      case "width":
      case "height":
        // (as Winamp: where the divider starts; a script's later setting
        // of it isn't heard, setPosition() is)
        if (!this._framed) this._position = num(value) || 0;
        break;
      case "minwidth":
      case "minheight":
        this._minwidth = num(value) || 0;
        this._layoutFrame();
        break;
      case "maxwidth":
      case "maxheight":
        this._maxwidth = String(value).toLowerCase() === "null" ? 0 : num(value) || 0;
        this._layoutFrame();
        break;
      case "resizable":
        this._resizable = (num(value) || 0) !== 0;
        this._layoutFrame();
        break;
      case "orientation": {
        const v = String(value).toLowerCase();
        if (v === "v" || v === "vertical") this._orientation = "v";
        if (v === "h" || v === "horizontal") this._orientation = "h";
        break;
      }
      case "from":
        this.setFrom(String(value).toLowerCase()[0]);
        break;
      case "left":
      case "top":
      case "right":
      case "bottom":
        this[`_${key}Id`] = value.toLowerCase();
        break;
      case "vbitmap":
        this._vbitmap = value;
        break;
      case "vgrabber":
        this._vgrabber = value;
        break;
      default:
        return false;
    }
    return true;
  }

  getposition(): number {
    return this._position;
  }
  setposition(position: number) {
    this._position = Math.max(0, Math.min(PULLBAR_FULL, Math.round(Number(position) || 0)));
    this._layoutFrame();
  }

  setFrom(from: string) {
    //? because maki expect to read 'left' instead 'l'
    const correction = {
      l: "left",
      r: "right",
      b: "bottom",
      t: "top",
    };
    this._from = correction[from] ?? this._from;
    this._fromRight = this._from === "right" || this._from === "bottom";
  }

  init() {
    super.init();
    this._framed = true;
    this._layoutFrame();
  }

  draw() {
    super.draw();
    if (!this._sizer) {
      this._sizer = document.createElement("div");
      this._sizer.className = "frame-sizer";
      this._sizer.style.cssText = "position:absolute;background-size:100% 100%;background-repeat:no-repeat;z-index:1";
      this._grabber = document.createElement("div");
      this._grabber.style.cssText = "position:absolute;background-size:100% 100%;background-repeat:no-repeat;pointer-events:none";
      this._sizer.appendChild(this._grabber);
      this._sizer.addEventListener("pointerdown", (e) => this._drag(e));
    }
    this._div.appendChild(this._sizer);
    if (!this._observer && typeof ResizeObserver !== "undefined") {
      this._observer = new ResizeObserver(() => this._layoutFrame());
      this._observer.observe(this._div);
    }
    this._layoutFrame();
  }

  dispose() {
    this._observer?.disconnect();
    this._observer = null;
    super.dispose?.();
  }

  /** A group put at a place in the frame (and told, when it moved or changed size). */
  _place(child: GuiObj | null, x: number, y: number, w: number, h: number) {
    if (!child) return;
    const at = [Math.round(x), Math.round(y), Math.max(0, Math.round(w)), Math.max(0, Math.round(h))];
    const changed = child._x !== at[0] || child._y !== at[1] || child._w !== at[2] || child._h !== at[3];
    child._relatx = child._relaty = child._relatw = child._relath = "0";
    child.resize(at[0], at[1], at[2], at[3]);
    if (changed) child.doResize?.();
  }

  _layoutFrame() {
    if (!this._div || !this._div.isConnected) return;
    const W = this._div.offsetWidth;
    const H = this._div.offsetHeight;
    if (W < 1 && H < 1) return;
    const a = this._first;
    const b = this._second;
    if (!a || !b) {
      this._place(a ?? b, 0, 0, W, H);
      this._showSizer(null);
      return;
    }
    const vertical = this._orientation !== "h";
    const sz = this._resizable ? SIZER : 0;
    const client = vertical ? W : H;
    let w = this._position;
    if (w === PULLBAR_FULL) w = client;
    else if (w === PULLBAR_FULL / 2) w = Math.floor(client / 2);
    else if (w === PULLBAR_FULL / 4) w = Math.floor(client / 4);
    else if (w === PULLBAR_FULL - PULLBAR_FULL / 4) w = client - Math.floor(client / 4);
    else if (w === PULLBAR_FULL / 8) w = Math.floor(client / 8);
    if (this._fromRight) {
      w = client - w;
      if (this._maxwidth < 1 && w < -this._maxwidth) w = -this._maxwidth;
    } else {
      if (this._maxwidth < 1 && w > client + this._maxwidth) w = client + this._maxwidth;
      if (w < this._minwidth) w = this._minwidth;
    }
    if (vertical) {
      if (!this._fromRight) {
        this._place(a, 0, 0, w, H);
        this._place(b, w + sz, 0, W - (w + sz), H);
        this._showSizer(sz ? [w, 0, sz, H] : null);
      } else {
        this._place(a, 0, 0, w - sz, H);
        this._place(b, w, 0, W - w, H);
        this._showSizer(sz ? [w - sz, 0, sz, H] : null);
      }
    } else {
      this._place(a, 0, 0, W, w);
      this._place(b, 0, w + sz, W, H - (w + sz));
      this._showSizer(sz ? [0, w, W, sz] : null);
    }
  }

  /** The divider: the skin's divider picture stretched over it (2 pixels
   *  past its ends), its grabber in the middle; none when not resizable. */
  _showSizer(rect: number[] | null) {
    const s = this._sizer;
    if (!s) return;
    if (!rect) {
      s.style.display = "none";
      return;
    }
    const vertical = this._orientation !== "h";
    const [x, y, w, h] = rect;
    s.style.display = "block";
    s.style.left = `${vertical ? x : x - 2}px`;
    s.style.top = `${vertical ? y - 2 : y}px`;
    s.style.width = `${vertical ? w : w + 4}px`;
    s.style.height = `${vertical ? h + 4 : h}px`;
    s.style.cursor = vertical ? "ew-resize" : "ns-resize";
    const bitmapOf = (id: string) => (id ? this._uiRoot.getBitmaps()[String(id).toLowerCase()] : null);
    const line = bitmapOf(vertical ? this._vbitmap : this._hbitmap);
    s.style.backgroundImage = line ? `var(${line.getCSSVar()})` : "none";
    const grab = bitmapOf(vertical ? this._vgrabber : this._hgrabber);
    const g = this._grabber;
    if (!g) return;
    if (!grab) {
      g.style.display = "none";
      return;
    }
    const gw = grab.getWidth();
    const gh = grab.getHeight();
    g.style.display = "block";
    g.style.backgroundImage = `var(${grab.getCSSVar()})`;
    if (vertical) {
      g.style.left = "0px";
      g.style.width = `${w}px`;
      g.style.top = `${2 + Math.max(0, (h - gh) / 2)}px`;
      g.style.height = `${Math.min(gh, h)}px`;
    } else {
      g.style.top = "0px";
      g.style.height = `${h}px`;
      g.style.left = `${2 + Math.max(0, (w - gw) / 2)}px`;
      g.style.width = `${Math.min(gw, w)}px`;
    }
  }

  /** Dragging the divider moves it (as Winamp's did, while resizable). */
  _drag(e: PointerEvent) {
    if (!this._resizable) return;
    e.preventDefault();
    e.stopPropagation();
    const vertical = this._orientation !== "h";
    const start = vertical ? e.clientX : e.clientY;
    const shown = this._div.getBoundingClientRect()[vertical ? "width" : "height"] || 1;
    const scale = shown / ((vertical ? this._div.offsetWidth : this._div.offsetHeight) || 1);
    const from = this._position;
    const move = (ev: PointerEvent) => {
      const delta = ((vertical ? ev.clientX : ev.clientY) - start) / scale;
      this.setposition(this._fromRight ? from - delta : from + delta);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }
}
