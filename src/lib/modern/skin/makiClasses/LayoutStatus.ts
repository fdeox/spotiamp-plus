// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import Group from "./Group";
import Layout from "./Layout";
import { drawWasabiButton } from "../wasabiPaint";
import { paintText, systemFace, wasabiTextAvailable } from "../wasabiText";

// http://wiki.winamp.com/wiki/XML_GUI_Objects#.3Clayoutstatus.2F.3E
/**
 * Spotiamp+: a window's status bar as Wasabi drew it (status.cpp): the
 * commands of what the window holds as buttons, the left ones from the left
 * and the right ones from the right (the playlist editor's Add, Rem, Sel,
 * Misc and List), the skin's wasabi.statusbar between them. A button is
 * Wasabi's (wasabi.button.*), as wide as its name with a "j" each side and a
 * tenth more, its name in the middle (ButtonWnd). Read for its behaviour;
 * none of Winamp's code is used.
 */
export default class LayoutStatus extends Group {
  static GUID = "7fd5f21048dfacc45154a0a676dc6c57";
  _exclude: string[] = [];
  _includeOnly: string[] = [];
  _buttons: { name: string; right: boolean; menu: string; x: number; w: number }[] = [];
  _canvas: HTMLCanvasElement = document.createElement("canvas");
  _pressed: string = "";
  _painting: number = 0;
  _resize: ResizeObserver | null = null;
  /** the skin's wasabi.statusbar, between the buttons */
  _bar: any = null;

  getElTag(): string {
    return "layoutstatus";
  }

  setXmlAttr(_key: string, value: string): boolean {
    const key = _key.toLowerCase();
    if (super.setXmlAttr(key, value)) {
      return true;
    }
    switch (key) {
      case "exclude":
        this._exclude = splitList(value);
        break;
      case "includeonly":
        this._includeOnly = splitList(value);
        break;
      default:
        return false;
    }
    return true;
  }

  draw() {
    super.draw();
    const c = this._canvas;
    c.style.position = "absolute";
    c.style.left = "0";
    c.style.top = "0";
    c.style.imageRendering = "pixelated";
    this._div.appendChild(c);
    this._div.style.pointerEvents = "auto";
    c.addEventListener("pointerdown", this._onDown);
    c.addEventListener("pointerup", this._onUp);
    c.addEventListener("pointerleave", () => this._press(""));
    this._resize = new ResizeObserver(() => this._paint());
    this._resize.observe(this._div);
  }

  init() {
    super.init();
    // (once the window's other parts are there)
    setTimeout(() => this._paint(), 0);
  }

  dispose() {
    this._resize?.disconnect();
    super.dispose?.();
  }

  /** a script pressing one of the buttons by its name (fakeButtonPush) */
  callme(str: string) {
    const b = this._commands().find((c) => c.name.toLowerCase() === String(str ?? "").toLowerCase());
    if (b) this._uiRoot.dispatch(b.menu);
  }

  /** what the window holds offers (the playlist editor's commands), as the
   *  exclude / includeonly lists leave them */
  _commands() {
    let layout = this._parent;
    while (layout && !(layout instanceof Layout)) layout = layout._parent;
    const find = (o, depth = 0) => {
      if (!o || depth > 30) return null;
      if (o._appCmds) return o._appCmds;
      for (const child of o._children ?? []) {
        const got = find(child, depth + 1);
        if (got) return got;
      }
      return null;
    };
    const cmds = find(layout) ?? [];
    const lower = (list: string[]) => list.map((s) => s.toLowerCase());
    const exclude = lower(this._exclude);
    const only = lower(this._includeOnly);
    return cmds.filter((c) => !exclude.includes(c.name.toLowerCase()) && (!only.length || only.includes(c.name.toLowerCase())));
  }

  _buttonTextColor(): [number, number, number] {
    try {
      const has = this._uiRoot._colors.some((c) => String(c._id).toLowerCase() === "wasabi.button.text");
      const rgb = has ? this._uiRoot._namedColorRgb("wasabi.button.text") : null;
      if (rgb) return rgb.map((v) => Math.round(v)) as [number, number, number];
    } catch {}
    return [0, 0, 0];
  }

  async _paint() {
    const w = Math.round(this._div?.offsetWidth ?? 0);
    const h = Math.round(this._div?.offsetHeight ?? 0);
    if (w < 1 || h < 1) return;
    const ticket = ++this._painting;
    const cmds = this._commands();
    const ui = this._uiRoot;
    const native = wasabiTextAvailable();
    const renderer = (ui._skinVersion ?? 0) >= 1.3 ? "gdi" : "freetype";
    const face = native ? await systemFace("arial") : "Arial";
    const color = this._buttonTextColor();
    // the name as Wasabi wrote it (its default font, 14), centred
    const text = (t: string, bw: number) =>
      paintText({
        face,
        size: 14,
        bold: 0,
        antialias: true,
        text: t,
        // (the text widget's 2 px margin taken off, to centre on the button)
        w: bw + 2,
        h,
        align: 1,
        valign: 1,
        lpadding: 0,
        rpadding: 0,
        offsetx: -2,
        offsety: 0,
        color,
        shadow: null,
        wrapped: false,
        time: null,
        renderer,
        ft_font: "sys:arial",
        ft_name: "arial",
      });
    const widths: number[] = [];
    for (const c of cmds) {
      let tw = (c.name.length + 2) * 6;
      if (native) {
        const m = await text(`j${c.name}j`, 1);
        if (m) tw = m.textW;
      }
      widths.push(Math.max(Math.trunc((tw * 11) / 10), 8));
    }
    if (ticket !== this._painting) return;
    const buttons = [];
    let x = 0;
    cmds.forEach((c, i) => {
      if (!c.right) {
        buttons.push({ ...c, x, w: widths[i] });
        x += widths[i];
      }
    });
    const rights = cmds.map((c, i) => ({ c, bw: widths[i] })).filter(({ c }) => c.right);
    x = w - rights.reduce((sum, r) => sum + r.bw, 0);
    for (const { c, bw } of rights) {
      buttons.push({ ...c, x, w: bw });
      x += bw;
    }
    // the names first, then all of it at once
    const names = [];
    for (const b of buttons) names.push(native ? await text(b.name, b.w) : null);
    if (ticket !== this._painting) return;
    this._buttons = buttons;
    // the skin's bar in the room the buttons leave
    const leftW = buttons.filter((b) => !b.right).reduce((sum, b) => sum + b.w, 0);
    const rightW = buttons.filter((b) => b.right).reduce((sum, b) => sum + b.w, 0);
    if (this._bar) {
      this._bar.setxmlparam("x", String(leftW));
      this._bar.setxmlparam("w", String(-(leftW + rightW)));
    }
    const canvas = this._canvas;
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    const ctx = canvas.getContext("2d");
    ctx.clearRect(0, 0, w, h);
    buttons.forEach((b, i) => {
      const pushed = this._pressed === b.name;
      drawWasabiButton(ui, ctx, b.x, 0, b.w, h, pushed);
      const painted = names[i];
      if (painted) {
        const tmp = document.createElement("canvas");
        tmp.width = painted.w;
        tmp.height = painted.h;
        tmp.getContext("2d").putImageData(new ImageData(new Uint8ClampedArray(painted.data), painted.w, painted.h), 0, 0);
        const shift = pushed ? 1 : 0;
        ctx.drawImage(tmp, b.x + shift, shift);
      } else if (!native) {
        ctx.fillStyle = `rgb(${color.join(",")})`;
        ctx.font = "11px Arial";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(b.name, b.x + b.w / 2, h / 2);
      }
    });
  }

  _at(e: PointerEvent) {
    const r = this._canvas.getBoundingClientRect();
    const x = ((e.clientX - r.left) * (this._canvas.width || 1)) / (r.width || 1);
    return this._buttons.find((b) => x >= b.x && x < b.x + b.w) ?? null;
  }

  _press(name: string) {
    if (this._pressed === name) return;
    this._pressed = name;
    this._paint();
  }

  _onDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    const b = this._at(e);
    if (!b) return;
    e.stopPropagation();
    this._press(b.name);
  };

  _onUp = (e: PointerEvent) => {
    const b = this._at(e);
    const was = this._pressed;
    this._press("");
    // its menu, as Winamp's playlist buttons popped theirs
    if (b && b.name === was) this._uiRoot.dispatch(b.menu);
  };
}

function splitList(value: string): string[] {
  return String(value ?? "")
    .split(";")
    .map((s) => s.trim())
    .filter(Boolean);
}
