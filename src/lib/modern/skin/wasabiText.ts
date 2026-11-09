// Spotiamp+: modern skins' text painted the way Winamp 5 painted it: Windows
// draws it with GDI, laid out as Wasabi's Text widget lays it out
// (src-tauri/src/wasabi_text.rs). In the app that's a command; a test page
// can name a tool built from the same code (window.__wasabiTextUrl). With
// neither, texts stay the browser's.
import { invoke } from "@tauri-apps/api/core";

export type TextPaint = {
  face: string;
  size: number;
  bold: number;
  antialias: boolean;
  text: string;
  w: number;
  h: number;
  align: number;
  valign: number;
  lpadding: number;
  rpadding: number;
  offsetx: number;
  offsety: number;
  color: [number, number, number];
  shadow: { x: number; y: number; color: [number, number, number] } | null;
  wrapped: boolean;
  time: { hours: boolean; fixed: boolean; colon: number } | null;
  /** "freetype" for a skin older than 1.3 (Winamp's FreeType renderer) */
  renderer: "gdi" | "freetype";
  /** FreeType's font: "face:<face>" (a skin's, loaded) or "sys:<id>" */
  ft_font: string;
  /** the skin font's file name without its extension, as that renderer knew it */
  ft_name: string;
};

export type PaintedText = { w: number; h: number; textW: number; suggested: number; data: Uint8ClampedArray };

function testUrl(): string | null {
  return typeof window !== "undefined" ? ((window as any).__wasabiTextUrl ?? null) : null;
}

function inApp(): boolean {
  return typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;
}

export function wasabiTextAvailable(): boolean {
  return inApp() || !!testUrl();
}

async function call(command: string, path: string, body: any, raw = false): Promise<any> {
  if (inApp()) return invoke(command, raw ? body : { [command === "wasabi_text_paint" ? "paint" : "id"]: body });
  const url = testUrl();
  if (!url) return null;
  const response = await fetch(url + path, { method: "POST", body: raw || typeof body === "string" ? body : JSON.stringify(body) });
  return command === "wasabi_text_paint" ? response.arrayBuffer() : response.text();
}

export async function paintText(paint: TextPaint): Promise<PaintedText | null> {
  let buffer: ArrayBuffer;
  try {
    buffer = await call("wasabi_text_paint", "/paint", paint);
  } catch {
    return null;
  }
  if (!buffer || buffer.byteLength < 16) return null;
  const view = new DataView(buffer);
  const w = view.getInt32(0, true);
  const h = view.getInt32(4, true);
  const textW = view.getInt32(8, true);
  const suggested = view.getInt32(12, true);
  if (w <= 0 || h <= 0 || buffer.byteLength < 16 + w * h * 4) return null;
  return { w, h, textW, suggested, data: new Uint8ClampedArray(buffer, 16, w * h * 4) };
}

const systemFaces = new Map<string, Promise<string>>();

/** The face Winamp drew a font the skin doesn't have with: Windows' own
 *  "<id>.ttf", else Arial. */
export function systemFace(id: string): Promise<string> {
  const key = String(id ?? "").toLowerCase();
  let face = systemFaces.get(key);
  if (!face) {
    face = call("wasabi_text_face", "/face", String(id ?? "")).then(
      (f) => f || "Arial",
      () => "Arial"
    );
    systemFaces.set(key, face);
  }
  return face;
}

/** A skin's font file loaded for its texts; its face ("" when it isn't one). */
export async function skinFace(bytes: ArrayBuffer): Promise<string> {
  try {
    return (await call("wasabi_text_font", "/font", new Uint8Array(bytes), true)) || "";
  } catch {
    return "";
  }
}
