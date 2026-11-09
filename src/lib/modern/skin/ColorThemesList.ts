// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import GuiObj from "./makiClasses/GuiObj";
import { UIRoot } from "../UIRoot";
import { removeAllChildNodes, toBool } from "../utils";
import { bitmapCanvas } from "./wasabiPaint";

/**
 * ColorThemes:List. Spotiamp+: as Winamp showed it, a Wasabi list: a
 * "Theme" column header, the skin's colour themes by name in order, a row
 * 14 px high in the list's colours, the selected one on the selection
 * colour (the theme on, to begin with), and the skin's scrollbar. A double
 * click, or Switch, puts the selected one on; Previous and Next the one
 * before or after it. (Webamp showed a browser list box.)
 */
const ROW_H = 14;
const HEADER_H = 12;

export default class ColorThemesList extends GuiObj {
  _nohscroll: boolean = false;
  _nocolheader: boolean = false;
  _names: { key: string; name: string }[] = [];
  _selected: number = -1;
  _offs: number = 0;
  _header: HTMLDivElement = document.createElement("div");
  _body: HTMLDivElement = document.createElement("div");
  _bar: HTMLCanvasElement = document.createElement("canvas");
  _resize: ResizeObserver | null = null;

  constructor(uiRoot: UIRoot) {
    super(uiRoot);
    this._div.append(this._header, this._body, this._bar);
  }

  // (its page element by a name of its own: a minified build shortens the
  // class's)
  getElTag(): string {
    return "colorthemeslist";
  }

  setXmlAttr(key: string, value: string): boolean {
    if (super.setXmlAttr(key, value)) {
      return true;
    }
    const _key = key.toLowerCase();
    switch (_key) {
      // see skin: D-Reliction
      case "nocolheader":
        this._nocolheader = toBool(value);
        break;
      case "nohscroll":
        this._nohscroll = toBool(value);
        break;
      default:
        return false;
    }
    return true;
  }

  _color(id: string, fallback: [number, number, number]): string {
    const ui = this._uiRoot;
    const has = ui._colors.some((c) => String(c._id).toLowerCase() === id);
    const rgb = has ? ui._namedColorRgb(id) : null;
    const [r, g, b] = rgb ?? fallback;
    return `rgb(${Math.round(r)},${Math.round(g)},${Math.round(b)})`;
  }

  _load() {
    const ui = this._uiRoot;
    this._names = [...ui._gammaSets.keys()]
      .map((key) => ({ key, name: String(ui._gammaNames?.[key] ?? key) }))
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
    const on = String(ui._activeGammaSetName ?? "").toLowerCase();
    this._selected = this._names.findIndex((n) => n.key.toLowerCase() === on || n.name.toLowerCase() === on);
  }

  _rowsShown(): number {
    return Math.max(1, Math.floor((this._body.clientHeight || 0) / ROW_H));
  }

  _render() {
    const div = this._div;
    const w = div.offsetWidth;
    const h = div.offsetHeight;
    if (w < 1 || h < 1) return;
    const barW = (bitmapCanvas(this._uiRoot, "wasabi.scrollbar.vertical.background")?.width ?? 0) || 13;
    const top = this._nocolheader ? 0 : HEADER_H + 3;
    Object.assign(this._header.style, {
      position: "absolute",
      left: "0",
      top: "0",
      width: `${w}px`,
      height: `${HEADER_H}px`,
      display: this._nocolheader ? "none" : "block",
      background: this._color("wasabi.list.column.background", [102, 102, 102]),
      color: this._color("wasabi.list.column.text", [255, 255, 255]),
      font: "11px Arial, sans-serif",
      fontWeight: "normal",
      lineHeight: `${HEADER_H}px`,
      paddingLeft: "4px",
      boxSizing: "border-box",
      overflow: "hidden",
      whiteSpace: "nowrap",
    });
    this._header.textContent = "Theme";
    Object.assign(this._body.style, {
      position: "absolute",
      left: "2px",
      top: `${top}px`,
      width: `${Math.max(0, w - barW - 2 - 11)}px`,
      height: `${Math.max(0, h - top)}px`,
      overflow: "hidden",
      font: "11px Arial, sans-serif",
      fontWeight: "normal",
    });
    const shown = this._rowsShown();
    const max = Math.max(0, this._names.length - shown);
    this._offs = Math.min(Math.max(0, this._offs), max);
    removeAllChildNodes(this._body);
    const text = this._color("wasabi.list.text", [255, 255, 255]);
    const selBg = this._color("wasabi.list.item.selected", [255, 255, 255]);
    const selFg = this._color("wasabi.list.item.selected.fg", [255, 255, 255]);
    for (let i = this._offs; i < this._names.length && i < this._offs + shown + 1; i++) {
      const row = document.createElement("div");
      const sel = i === this._selected;
      Object.assign(row.style, {
        position: "absolute",
        left: "0",
        top: `${(i - this._offs) * ROW_H}px`,
        width: "100%",
        height: `${ROW_H}px`,
        lineHeight: `${ROW_H}px`,
        paddingLeft: "2px",
        boxSizing: "border-box",
        whiteSpace: "nowrap",
        overflow: "hidden",
        color: sel ? selFg : text,
        background: sel ? selBg : "transparent",
        cursor: "default",
      });
      row.textContent = this._names[i].name;
      row.addEventListener("mousedown", () => {
        this._selected = i;
        this._render();
      });
      row.addEventListener("dblclick", () => this.handleAction("colorthemes_switch"));
      this._body.appendChild(row);
    }
    this._paintBar(w, h, barW, top);
  }

  /** the skin's vertical scrollbar: its background, its arrows, its button */
  _paintBar(w: number, h: number, barW: number, top: number) {
    const ui = this._uiRoot;
    const bar = this._bar;
    const bh = Math.max(0, h - top);
    bar.width = barW;
    bar.height = Math.max(1, bh);
    Object.assign(bar.style, { position: "absolute", left: `${w - barW}px`, top: `${top}px`, width: `${barW}px`, height: `${bh}px` });
    const ctx = bar.getContext("2d");
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, barW, bh);
    const pic = (id) => bitmapCanvas(ui, id);
    const bg = pic("wasabi.scrollbar.vertical.background") ?? pic("wasabi.scrollbar.vertical.background.middle");
    if (bg && bg.width && bg.height) ctx.drawImage(bg, 0, 0, bg.width, bg.height, 0, 0, barW, bh);
    const up = pic("wasabi.scrollbar.vertical.left");
    const down = pic("wasabi.scrollbar.vertical.right");
    const upH = up?.height ?? 0;
    const downH = down?.height ?? 0;
    if (up && up.width) ctx.drawImage(up, 0, 0);
    if (down && down.width) ctx.drawImage(down, 0, bh - downH);
    const button = pic("wasabi.scrollbar.vertical.button");
    const shown = this._rowsShown();
    const max = Math.max(0, this._names.length - shown);
    if (button && button.width && max > 0) {
      const track = bh - upH - downH;
      const thumbH = Math.max(button.height, Math.round((track * shown) / this._names.length));
      const y = upH + Math.round(((track - thumbH) * this._offs) / max);
      ctx.drawImage(button, 0, 0, button.width, button.height, 0, y, barW, thumbH);
    }
  }

  handleAction(action: string, param: string | null = null, actionTarget: string | null = null) {
    switch (action) {
      case "colorthemes_switch": {
        const pick = this._names[this._selected];
        if (pick) {
          this._uiRoot.enableGammaSet(pick.key);
          this._render();
        }
        return true;
      }
      case "colorthemes_previous":
        if (this._selected > 0) {
          this._selected--;
          return this.handleAction("colorthemes_switch");
        }
        break;
      case "colorthemes_next":
        if (this._selected < this._names.length - 1) {
          this._selected++;
          return this.handleAction("colorthemes_switch");
        }
        break;
    }
    return false;
  }

  draw() {
    super.draw();
    this._div.classList.add("list");
    this._div.setAttribute("data-obj-name", "ColorThemes:List");
    this._div.style.pointerEvents = "auto";
    this._load();
    this._div.addEventListener(
      "wheel",
      (e: WheelEvent) => {
        e.preventDefault();
        this._offs += e.deltaY > 0 ? 3 : -3;
        this._render();
      },
      { passive: false }
    );
    this._uiRoot.on?.("colorthemechanged", () => {
      this._load();
      this._render();
    });
    this._resize = new ResizeObserver(() => this._render());
    this._resize.observe(this._div);
    this._render();
  }

  dispose() {
    this._resize?.disconnect();
    super.dispose?.();
  }
}
