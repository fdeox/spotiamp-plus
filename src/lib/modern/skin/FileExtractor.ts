// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
/**
 * Below classes is mere not needed when we only work with zip/compressed file
 * (which is natively supported by webamp-modern),
 * but we made it like this to support more wider range of skins file format.
 */

import JSZip from "jszip";
import { getCaseInsensitiveFile } from "../utils";
// Spotiamp+: the files of Winamp's own folder that skins use (ClassicPro's
// engine, for its skins), packed with the app
import winampFilesUrl from "../winamp/winamp-files.zip?url";

/**
 * Special class that do only extract files from a skin file
 */
export abstract class FileExtractor {
  //

  abstract prepare(skinPath: string, response: Response): Promise<void>;

  abstract getFileAsString(filePath: string): Promise<string>;
  abstract getFileAsBytes(filePath: string): Promise<ArrayBuffer>;
  abstract getFileAsBlob(filePath: string): Promise<Blob>;
}

/**
 * Works only with Zip
 */
export class ZipFileExtractor {
  _zip: JSZip;
  /** Spotiamp+: the skin's own folder in the zip ("" at its top, or
   *  "Bento/" when it holds a few skins' folders), and its files by their
   *  lowercased paths */
  _root: string = "";
  _files: Map<string, any> = new Map();
  /** Spotiamp+: Winamp's own folder (its plugins' files), by lowercased paths */
  _winamp: Map<string, any> = new Map();
  /** Spotiamp+: the skin's file, for its name */
  _skinFile: string = "";

  async prepare(skinPath: string, response: Response) {
    this._skinFile = decodeURIComponent(String(skinPath ?? "").split("?")[0]);
    const skinZipBlob = await response.blob();
    this._zip = await JSZip.loadAsync(skinZipBlob);
    this._winamp = await winampFolder();
    this._files = new Map();
    this._zip.forEach((path, file) => {
      if (!file.dir) this._files.set(path.replace(/\\/g, "/").toLowerCase(), file);
    });
    // the skin is where the shallowest skin.xml is (the first, if several)
    let best = null;
    for (const path of this._files.keys()) {
      if (path !== "skin.xml" && !path.endsWith("/skin.xml")) continue;
      const depth = path.split("/").length;
      if (best == null || depth < best.depth) best = { path, depth };
    }
    this._root = best ? best.path.slice(0, -"skin.xml".length) : "";
  }

  /** The file a skin means by `filePath`: from its own folder, with "../"
   *  and Winamp's @SKINSPATH@ (the skins folder) worked out; else as
   *  Webamp looked for it, anywhere a path ends that way. */
  _find(filePath: string) {
    let path = filePath.replace(/\\/g, "/").replace(/^\.\//, "");
    // Winamp's own folder (@WINAMPPATH@, and @COLORTHEMESPATH@, two folders
    // into it: Plugins/freeform)
    const inWinamp = winampPath(path);
    if (inWinamp != null) return this._winamp.get(inWinamp) ?? null;
    let base = this._root;
    if (/^@skinspath@\//i.test(path)) {
      path = path.replace(/^@skinspath@\//i, "");
      base = "";
      // the skins folder is the one above this skin's
      const parts = this._root.split("/").filter(Boolean);
      parts.pop();
      if (parts.length) base = parts.join("/") + "/";
    }
    const parts = (base + path).split("/");
    const resolved = [];
    for (const part of parts) {
      if (part === "..") resolved.pop();
      else if (part && part !== ".") resolved.push(part);
    }
    const exact = this._files.get(resolved.join("/").toLowerCase());
    if (exact) return exact;
    // another skin's folder that isn't in the zip: its file, if this has one
    const loose = path.replace(/^(\.\.\/)+[^/]+\//, "");
    // else Winamp's own, as Wasabi looked there next (Plugins/freeform: its
    // xml/winamp/cover pictures for a song with none)
    const rel = this._resolve("", path);
    // (and Wasabi's own folder, whose elements name their files from it:
    // window/background.png is the skin's when it has one, else Wasabi's)
    return (
      getCaseInsensitiveFile(this._zip, loose) ??
      this._winamp?.get(("plugins/freeform/" + rel).toLowerCase()) ??
      this._winamp?.get(("plugins/freeform/xml/wasabi/" + rel).toLowerCase()) ??
      null
    );
  }

  /**
   * Spotiamp+: the files a wildcard names, as Winamp's includes allowed
   * ("widgets/load/*.xml"): their paths, the way they'd be asked for.
   */
  list(pattern: string): string[] {
    const path = pattern.replace(/\\/g, "/").replace(/^\.\//, "");
    const slash = path.lastIndexOf("/");
    const dir = slash >= 0 ? path.slice(0, slash + 1) : "";
    const glob = new RegExp(
      "^" + path.slice(slash + 1).replace(/[.+^${}()|[\]]/g, "\\$&").replace(/\*/g, "[^/]*").replace(/\?/g, "[^/]") + "$",
      "i",
    );
    const inWinamp = winampPath(dir + "x");
    const folder = inWinamp != null ? inWinamp.slice(0, -1) : this._resolve(this._root, dir + "x").slice(0, -1);
    const files = inWinamp != null ? this._winamp : this._files;
    const out: string[] = [];
    for (const key of files.keys()) {
      if (!key.startsWith(folder.toLowerCase())) continue;
      const name = key.slice(folder.length);
      if (!name.includes("/") && glob.test(name)) out.push(dir + name);
    }
    return out.sort();
  }

  /** a path from `base`, its ".." worked out, lowercased */
  _resolve(base: string, path: string): string {
    const resolved = [];
    for (const part of (base + path).split("/")) {
      if (part === "..") resolved.pop();
      else if (part && part !== ".") resolved.push(part);
    }
    return resolved.join("/").toLowerCase();
  }

  async getFileAsString(filePath: string): Promise<string> {
    if (!filePath) return null;
    const zipObj = this._find(filePath);
    if (!zipObj) return null;
    // Spotiamp+: UTF-16 too, as Windows' Notepad saved some skins' XML
    const bytes: Uint8Array = await zipObj.async("uint8array").catch(() => null);
    if (!bytes) return null;
    if (bytes[0] === 0xff && bytes[1] === 0xfe) return new TextDecoder("utf-16le").decode(bytes.subarray(2));
    if (bytes[0] === 0xfe && bytes[1] === 0xff) return new TextDecoder("utf-16be").decode(bytes.subarray(2));
    return new TextDecoder("utf-8").decode(bytes);
  }

  async getFileAsBytes(filePath: string): Promise<ArrayBuffer> {
    if (!filePath) return null;
    const zipObj = this._find(filePath);
    if (!zipObj) return null;
    // (Spotiamp+: a damaged file in the zip is one that isn't there)
    return await zipObj.async("arraybuffer").catch(() => null);
  }

  async getFileAsBlob(filePath: string): Promise<Blob> {
    if (!filePath) return null;
    const zipObj = this._find(filePath);
    if (!zipObj) return null;
    return await zipObj.async("blob").catch(() => null);
  }
}

/**
 * Works only with path (url)
 * It do same as zip, but with path based world
 */
export class PathFileExtractor {
  _skinDir: string;

  async prepare(skinPath: string, response: Response) {
    if (!skinPath.endsWith("/")) skinPath += "/";
    this._skinDir = skinPath;
  }

  async getFileAsString(filePath: string): Promise<string> {
    const response = await fetch(this._skinDir + filePath);
    return await response.text();
  }

  async getFileAsBytes(filePath: string): Promise<ArrayBuffer> {
    const response = await fetch(this._skinDir + filePath);
    return await response.arrayBuffer();
  }

  async getFileAsBlob(filePath: string): Promise<Blob> {
    const response = await fetch(this._skinDir + filePath);
    return await response.blob();
  }
}

/** Spotiamp+: Winamp's own folder (packed with the app), read once. */
let winampFolderLoaded: Promise<Map<string, any>> | null = null;
function winampFolder(): Promise<Map<string, any>> {
  winampFolderLoaded ??= (async () => {
    const files = new Map();
    try {
      const zip = await JSZip.loadAsync(await (await fetch(winampFilesUrl)).arrayBuffer());
      zip.forEach((path, file) => {
        if (!file.dir) files.set(path.toLowerCase(), file);
      });
    } catch (e) {
      console.warn(`Winamp's own files not read: ${e}`);
    }
    return files;
  })();
  return winampFolderLoaded;
}

/** Spotiamp+: where a path into Winamp's own folder leads in it (lowercased), or null for the skin's own. */
function winampPath(path: string): string | null {
  let rest: string;
  if (/^@winamppath@/i.test(path)) rest = path.replace(/^@winamppath@\/?/i, "");
  else if (/^@colorthemespath@/i.test(path)) rest = "plugins/freeform/" + path.replace(/^@colorthemespath@\/?/i, "");
  else return null;
  const resolved = [];
  for (const part of rest.split("/")) {
    if (part === "..") resolved.pop();
    else if (part && part !== ".") resolved.push(part);
  }
  let out = resolved.join("/").toLowerCase();
  // ("@COLORTHEMESPATHPlugins\..." as some skins wrote it: from the plugins on)
  const plugins = out.indexOf("plugins/", 1);
  if (!out.startsWith("plugins/") && plugins > 0) out = out.slice(plugins);
  else if (out.startsWith("plugins/freeform/plugins/")) out = out.slice("plugins/freeform/".length);
  return out;
}
