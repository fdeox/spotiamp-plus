// Spotiamp+: a playlist's rows painted the way Winamp 5's playlist editor
// painted them (src-tauri/src/winamp_pe.rs): Windows draws each row with the
// playlist font, as Winamp's draw_pe_song did. In the app that's a command;
// a test page can name a tool built from the same code
// (window.__wasabiTextUrl). With neither, the page draws them itself.
import { invoke } from "@tauri-apps/api/core";

export type PeRow = { name: string; time: number; sel: number; fg?: [number, number, number] | null };

export type PePaint = {
  face: string;
  size: number;
  /** the list's width (the window's - 32) and height (- 58) */
  w: number;
  h: number;
  rows: PeRow[];
  /** normal text, the playing row's, background, selected background */
  colors: [number, number, number][];
};

export type PePainted = { w: number; h: number; rowH: number; rows: number; data: Uint8ClampedArray };

function testUrl(): string | null {
  return typeof window !== "undefined" ? ((window as any).__wasabiTextUrl ?? null) : null;
}

function inApp(): boolean {
  return typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;
}

export function winampPeAvailable(): boolean {
  return inApp() || !!testUrl();
}

export async function paintPe(paint: PePaint): Promise<PePainted | null> {
  let buffer: ArrayBuffer | null = null;
  try {
    if (inApp()) buffer = await invoke("winamp_pe_paint", { paint });
    else if (testUrl()) buffer = await (await fetch(testUrl() + "/pe", { method: "POST", body: JSON.stringify(paint) })).arrayBuffer();
  } catch {
    return null;
  }
  if (!buffer || buffer.byteLength < 16) return null;
  const view = new DataView(buffer);
  const w = view.getInt32(0, true);
  const h = view.getInt32(4, true);
  const rowH = view.getInt32(8, true);
  const rows = view.getInt32(12, true);
  if (w <= 0 || h <= 0 || buffer.byteLength < 16 + w * h * 4) return null;
  return { w, h, rowH, rows, data: new Uint8ClampedArray(buffer, 16, w * h * 4) };
}

/** " m:ss ", as Winamp wrote a row's length */
export function lengthText(time: number): string {
  return ` ${Math.floor(time / 60)}:${String(time % 60).padStart(2, "0")} `;
}

/** A row's name as Winamp's playlist showed it: its number, padded with
 *  spaces to the digits of the list's length (PlayList_getitem_pl). */
export function rowName(index: number, count: number, title: string): string {
  const digits = count >= 10000 ? 5 : count >= 1000 ? 4 : count >= 100 ? 3 : count >= 10 ? 2 : 1;
  return `${String(index + 1).padStart(digits, " ")}. ${title}`;
}

export type LinePainted = { w: number; h: number; data: Uint8ClampedArray };

/** A line of text as Winamp 5's main window wrote the song's title in a
 *  font (src-tauri/src/winamp_pe.rs, paint_line): its colour on the
 *  background, as wide as the text, as high as the font. */
export async function paintLine(paint: { face: string; size: number; text: string; fg: number[]; bg: number[] }): Promise<LinePainted | null> {
  let buffer: ArrayBuffer | null = null;
  try {
    if (inApp()) buffer = await invoke("winamp_line_paint", { paint });
    else if (testUrl()) buffer = await (await fetch(testUrl() + "/line", { method: "POST", body: JSON.stringify(paint) })).arrayBuffer();
  } catch {
    return null;
  }
  if (!buffer || buffer.byteLength < 16) return null;
  const view = new DataView(buffer);
  const w = view.getInt32(0, true);
  const h = view.getInt32(4, true);
  if (w <= 0 || h <= 0 || buffer.byteLength < 16 + w * h * 4) return null;
  return { w, h, data: new Uint8ClampedArray(buffer, 16, w * h * 4) };
}
