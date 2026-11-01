// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
/**
 * Below classes is mere not needed when we only work with zip/compressed file
 * (which is natively supported by webamp-modern),
 * but we made it like this to support more wider range of skins file format.
 */

import JSZip from "jszip";
import { getCaseInsensitiveFile } from "../utils";

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

  async prepare(skinPath: string, response: Response) {
    const skinZipBlob = await response.blob();
    this._zip = await JSZip.loadAsync(skinZipBlob);
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
    return getCaseInsensitiveFile(this._zip, loose);
  }

  async getFileAsString(filePath: string): Promise<string> {
    if (!filePath) return null;
    const zipObj = this._find(filePath);
    if (!zipObj) return null;
    return await zipObj.async("text");
  }

  async getFileAsBytes(filePath: string): Promise<ArrayBuffer> {
    if (!filePath) return null;
    const zipObj = this._find(filePath);
    if (!zipObj) return null;
    return await zipObj.async("arraybuffer");
  }

  async getFileAsBlob(filePath: string): Promise<Blob> {
    if (!filePath) return null;
    const zipObj = this._find(filePath);
    if (!zipObj) return null;
    return await zipObj.async("blob");
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
