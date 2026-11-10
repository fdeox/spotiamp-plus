// @ts-nocheck: written against the engine's loose types, like the rest of it
// Spotiamp+: the GENEX.BMP a modern skin gives Winamp 5's Media Library,
// made the way gen_ff made it (ff_ipc.cpp, ff_genwa2skinbitmap): a 132x75
// sheet in the window's colour, the skin's button at 47x15 normal and
// pressed, its arrows and scrollbar grips on 14 px buttons, and the 24
// colours along the top row worked out from the skin's own (the list's,
// the window's, the columns'...), with gen_ff's guesses where it has none.
// For skins of version 1.3 and up Winamp drew the scrollbars from the
// skin's own pictures (freeform), so those go along too. Read for its
// behaviour; none of Winamp's code is used.
import { bitmapCanvas, drawWasabiButton } from "./wasabiPaint";

type Rgb = [number, number, number];

/** what Winamp's own Wasabi skin gave a colour a skin doesn't name (wasabi.xml's values) */
const BASE: Record<string, Rgb> = {
  "wasabi.edit.text": [196, 255, 196],
  "wasabi.edit.background": [0, 0, 0],
  "wasabi.tree.text": [196, 255, 196],
  "wasabi.tree.selected": [0, 0, 128],
  "wasabi.list.text": [196, 255, 196],
  "wasabi.list.item.selected": [0, 0, 128],
  "wasabi.list.column.background": [102, 102, 102],
  "wasabi.list.column.text": [255, 255, 255],
  "wasabi.button.text": [0, 0, 0],
};

/** Blenders::BLEND_AVG, a byte at a time */
const avg = (a: Rgb, b: Rgb): Rgb => [(a[0] >> 1) + (b[0] >> 1), (a[1] >> 1) + (b[1] >> 1), (a[2] >> 1) + (b[2] >> 1)];
/** gen_ff's lumidiff: its MIN((r, g), b) kept only the last two bytes (green and blue) */
const lumidiff = (a: Rgb, b: Rgb) => Math.min(Math.abs(a[1] - b[1]), Math.abs(a[2] - b[2]));
const grey = (v: number): Rgb => [v, v, v];
const BLACK = grey(0);
const WHITE = grey(255);

export function makeMlGenex(ui): { genex: string; ff: Record<string, string> | null; menu: Record<string, Rgb> } | null {
  try {
    const version = ui._skinVersion ?? 1;
    const interpolate = version * 10 < 10;
    const has = (id: string) => ui._colors.some((c) => String(c._id).toLowerCase() === id.toLowerCase());
    /** SkinColor: the skin's (in the colour theme on), else the base's */
    const sc = (id: string): Rgb => {
      if (has(id)) {
        const rgb = ui._namedColorRgb(id);
        if (rgb) return [rgb[0], rgb[1], rgb[2]];
      }
      return BASE[id] ?? [255, 0, 255];
    };
    const pixelOf = (id: string, centre: boolean): Rgb | null => {
      const c = bitmapCanvas(ui, id);
      if (!c || !c.width || !c.height) return null;
      const d = c.getContext("2d", { willReadFrequently: true }).getImageData(centre ? c.width >> 1 : 0, centre ? c.height >> 1 : 0, 1, 1).data;
      return [d[0], d[1], d[2]];
    };
    /** skin_getBitmapColor: the pixel Wasabi read (its stride one short), premultiplied */
    const bitmapColor = (id: string): Rgb => {
      const c = bitmapCanvas(ui, id);
      if (!c || !c.width || !c.height) return BLACK;
      const w = c.width;
      const h = c.height;
      const big = document.createElement("canvas");
      big.width = w + 1;
      big.height = h + 1;
      const ctx = big.getContext("2d", { willReadFrequently: true });
      ctx.drawImage(c, 0, 0);
      const i = (w >> 1) + (h >> 1) * w + 1;
      const [r, g, b, a] = ctx.getImageData(i % (w + 1), Math.floor(i / (w + 1)), 1, 1).data;
      const pm = (v: number) => Math.round((v * a) / 255);
      return [pm(r), pm(g), pm(b)];
    };
    // getWindowBackground: the skin's window colour, else a window of it (its frame texture)
    const wbg: Rgb = has("wasabi.window.background")
      ? sc("wasabi.window.background")
      : (pixelOf("wasabi.frame.basetexture", true) ?? pixelOf("wasabi.basetexture", false) ?? grey(100));

    const canvas = document.createElement("canvas");
    canvas.width = 132;
    canvas.height = 75;
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    ctx.fillStyle = `rgb(${wbg.join(",")})`;
    ctx.fillRect(0, 0, 132, 75);
    // the "nothing here" colour along the top two rows, for the colour reader
    ctx.fillStyle = "rgb(0,198,255)";
    ctx.fillRect(47, 0, 85, 2);
    drawWasabiButton(ui, ctx, 0, 0, 47, 15, false);
    drawWasabiButton(ui, ctx, 0, 15, 47, 15, true);
    const arrows: [string, number, number, boolean][] = [
      ["wasabi.button.label.arrow.up", 0, 31, false],
      ["wasabi.button.label.arrow.down", 14, 31, false],
      ["wasabi.button.label.arrow.up", 28, 31, true],
      ["wasabi.button.label.arrow.down", 42, 31, true],
      ["wasabi.button.label.arrow.left", 0, 45, false],
      ["wasabi.button.label.arrow.right", 14, 45, false],
      ["wasabi.button.label.arrow.left", 28, 45, true],
      ["wasabi.button.label.arrow.right", 42, 45, true],
    ];
    for (const [id, x, y, pressed] of arrows) drawWasabiButton(ui, ctx, x, y, 14, 14, pressed, id);
    drawWasabiButton(ui, ctx, 56, 31, 14, 28, false, "wasabi.scrollbar.vertical.grip");
    drawWasabiButton(ui, ctx, 70, 31, 14, 28, true, "wasabi.scrollbar.vertical.grip");
    drawWasabiButton(ui, ctx, 84, 31, 28, 14, false, "wasabi.scrollbar.horizontal.grip");
    drawWasabiButton(ui, ctx, 84, 45, 28, 14, true, "wasabi.scrollbar.horizontal.grip");

    // the colours (in the sheet's order from x 48)
    const out: Record<number, Rgb> = {};
    const listbkg: Rgb = has("wasabi.list.background") ? sc("wasabi.list.background") : bitmapColor("wasabi.list.background");
    out[48] = listbkg;
    out[50] = has("wasabi.list.text") ? sc("wasabi.list.text") : sc("wasabi.edit.text");
    if (interpolate) {
      let c = out[50];
      c = lumidiff(c, listbkg) < 0x1f ? avg(out[50], grey(0x7f)) : c;
      c = lumidiff(c, listbkg) < 0x1f ? avg(out[50], grey(0x10)) : c;
      out[50] = c;
    }
    out[52] = wbg;
    out[54] = sc("wasabi.button.text");
    out[56] = !interpolate && has("wasabi.window.text") ? sc("wasabi.window.text") : sc("wasabi.list.text");
    if (!interpolate && has("wasabi.border.sunken")) out[58] = sc("wasabi.border.sunken");
    else out[58] = avg(wbg, Math.max(...wbg) > 0xe0 ? BLACK : WHITE);
    let col = sc("wasabi.list.column.background");
    if (interpolate && Math.max(...col) < 0x1f) col = avg(wbg, BLACK);
    out[62] = col;
    out[64] = sc("wasabi.list.column.text");
    const opt = (id: string, fallback: Rgb) => (!interpolate && has(id) ? sc(id) : fallback);
    out[66] = opt("wasabi.list.column.frame.top", avg(col, WHITE));
    out[68] = opt("wasabi.list.column.frame.middle", avg(col, grey(0x2f)));
    out[70] = opt("wasabi.list.column.frame.bottom", avg(col, BLACK));
    const empty = opt("wasabi.list.column.empty", avg(col, BLACK));
    out[72] = empty;
    out[74] = opt("wasabi.scrollbar.foreground", empty);
    out[76] = opt("wasabi.scrollbar.background", empty);
    out[78] = opt("wasabi.scrollbar.foreground.inverted", col);
    out[80] = opt("wasabi.scrollbar.background.inverted", empty);
    out[82] = wbg;
    let selfg: Rgb;
    if (has("wasabi.list.text.selected")) selfg = sc("wasabi.list.text.selected");
    else if (has("wasabi.list.text")) selfg = sc("wasabi.list.text");
    else {
      // (gen_ff left this one in the other byte order)
      const e = sc("wasabi.edit.text");
      selfg = [e[2], e[1], e[0]];
    }
    out[84] = selfg;
    let selbg: Rgb;
    if (has("wasabi.list.text.selected.background")) selbg = sc("wasabi.list.text.selected.background");
    else {
      selbg = has("wasabi.list.item.selected") ? sc("wasabi.list.item.selected") : sc("wasabi.tree.selected");
      const steps: [Rgb, Rgb][] = [
        [wbg, grey(0x7f)],
        [listbkg, grey(0x7f)],
        [wbg, grey(0xf0)],
        [listbkg, grey(0xf0)],
        [col, grey(0xf0)],
        [col, grey(0x10)],
      ];
      for (const [a, b] of steps) selbg = lumidiff(selbg, listbkg) < 0x2f ? avg(a, b) : selbg;
      selbg = lumidiff(selbg, selfg) < 0x1f ? avg(selbg, grey(0x10)) : selbg;
      selbg = lumidiff(selbg, selfg) < 0x1f ? avg(selbg, grey(0xf0)) : selbg;
    }
    out[86] = out[60] = selbg;
    out[88] = opt("wasabi.list.text.selected.inactive", selfg);
    // Blenders::BLEND_ADJ1 toward black, 210 of 255
    out[90] = opt("wasabi.list.text.selected.background.inactive", selbg.map((v) => Math.floor((v * 210) / 255)) as Rgb);
    out[92] = has("wasabi.list.background.alternate") ? sc("wasabi.list.background.alternate") : out[48];
    if (has("wasabi.list.text.alternate")) {
      // (gen_ff put it over the normal text's, and left the alternate unset)
      out[50] = sc("wasabi.list.text.alternate");
    } else out[94] = out[50];
    for (const [x, c] of Object.entries(out)) {
      ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`;
      ctx.fillRect(Number(x), 0, 1, 1);
    }

    // freeform scrollbars: the skin's own pictures (skins of 1.3 and up)
    let ff: Record<string, string> | null = null;
    if (version >= 1.3) {
      const ids = [
        "wasabi.scrollbar.vertical.left",
        "wasabi.scrollbar.vertical.right",
        "wasabi.scrollbar.vertical.left.pressed",
        "wasabi.scrollbar.vertical.right.pressed",
        "wasabi.scrollbar.vertical.button",
        "wasabi.scrollbar.vertical.button.pressed",
        "wasabi.scrollbar.horizontal.left",
        "wasabi.scrollbar.horizontal.right",
        "wasabi.scrollbar.horizontal.left.pressed",
        "wasabi.scrollbar.horizontal.right.pressed",
        "wasabi.scrollbar.horizontal.button",
        "wasabi.scrollbar.horizontal.button.pressed",
      ];
      const found: Record<string, string> = {};
      for (const id of ids) {
        const c = bitmapCanvas(ui, id);
        if (c && c.width && c.height) found[id] = c.toDataURL();
      }
      const bg = (names: string[]) => {
        for (const n of names) {
          const c = bitmapCanvas(ui, n);
          if (c && c.width && c.height) return c.toDataURL();
        }
        return null;
      };
      const vbg = bg(["wasabi.scrollbar.vertical.background.middle", "wasabi.scrollbar.vertical.background"]);
      const hbg = bg(["wasabi.scrollbar.horizontal.background.middle", "wasabi.scrollbar.horizontal.background"]);
      if (vbg) found.v_bg = vbg;
      if (hbg) found.h_bg = hbg;
      if (found["wasabi.scrollbar.vertical.left"] && found["wasabi.scrollbar.vertical.button"]) ff = found;
    }
    // the menus' colours, those the skin names (gen_ml asked the skin for them first)
    const menu: Record<string, Rgb> = {};
    const menuIds: [string, string][] = [
      ["bg", "wasabi.popupmenu.background"],
      ["sel", "wasabi.popupmenu.background.selected"],
      ["text", "wasabi.popupmenu.text"],
      ["textSel", "wasabi.popupmenu.text.selected"],
      ["textOff", "wasabi.popupmenu.text.inactive"],
      ["frame", "wasabi.popupmenu.frame"],
      ["separator", "wasabi.popupmenu.separator"],
    ];
    for (const [k, id] of menuIds) if (has(id)) menu[k] = sc(id);
    return { genex: canvas.toDataURL(), ff, menu };
  } catch {
    return null;
  }
}
