// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import * as Utils from "../utils";
import ImageManager from "./ImageManager";
import { skinFace } from "./wasabiText";

/**
 * Spotiamp+: a font a skin names that it doesn't have, as Winamp found it in
 * Windows' fonts: by its file's name (trebuc.ttf) or its own, and Winamp's
 * default font for "wasabi.font.default".
 */
const WINDOWS_FONTS: Record<string, string> = {
  arial: "Arial", arialbd: "Arial", ariali: "Arial", arialbi: "Arial", ariblk: "Arial Black",
  verdana: "Verdana", verdanab: "Verdana", tahoma: "Tahoma", tahomabd: "Tahoma",
  trebuc: "Trebuchet MS", trebucbd: "Trebuchet MS", impact: "Impact", georgia: "Georgia",
  times: "Times New Roman", timesbd: "Times New Roman", cour: "Courier New", courbd: "Courier New",
  comic: "Comic Sans MS", comicbd: "Comic Sans MS", lucon: "Lucida Console", l_10646: "Lucida Sans Unicode",
  segoeui: "Segoe UI", calibri: "Calibri", consola: "Consolas", micross: "Microsoft Sans Serif",
  gara: "Garamond", garamond: "Garamond", pala: "Palatino Linotype", framd: "Franklin Gothic Medium",
  frank: "Franklin Gothic Medium", sylfaen: "Sylfaen", webdings: "Webdings", wingding: "Wingdings",
  "wasabi.font.default": "Arial", "wasabi.font.button.small": "Arial", "wasabi.font.button.normal": "Arial",
};
export function windowsFontFamily(name: string): string {
  const key = String(name ?? "").toLowerCase().split(/[\\/]/).pop().replace(/\.(ttf|otf|fon)$/, "").trim();
  return WINDOWS_FONTS[key] ?? (String(name ?? "").replace(/\.(ttf|otf|fon)$/i, "") || "Arial");
}

export default class TrueTypeFont {
  _file: string;
  _id: string;
  _fontFace: FontFace;
  _inlineFamily: string;
  _imageManager: ImageManager;

  setXmlAttributes(attributes: { [attrName: string]: string }) {
    for (const [key, value] of Object.entries(attributes)) {
      this.setXmlAttr(key, value);
    }
  }

  setXmlAttr(key: string, value: string) {
    switch (key.toLowerCase()) {
      case "id":
        this._id = value;
        break;
      case "family":
        this._inlineFamily = value;
        break;
      case "file":
        this._file = value;
        break;
      default:
        return false;
    }
    return true;
  }

  getId() {
    return this._id || "";
  }

  // (Spotiamp+: a family CSS can match: a list as it is, one name quoted
  // once; a loaded face's family is already quoted when it has spaces, and
  // quoting it again made every skin font fall back to Times)
  getFontFamily() {
    const inline = this._inlineFamily;
    if (inline) return /[,'"]/.test(inline) ? inline : `"${inline}"`;
    const face = this._fontFace?.family;
    if (face) return /^["']/.test(face) ? face : `"${face}"`;
    return `"${String(this.getId()).replace(/"/g, "")}"`;
  }

  dispose() {
    (document.fonts as any).delete(this._fontFace);
  }

  getBase64() {
    console.log("getting Base64. me:", this.getId());
    return this._imageManager.getCachedUrl(this._file);
  }

  // dummy ttf has no url, such as Arial
  hasUrl(): boolean {
    return this._imageManager != null;
  }

  // Spotiamp+: the file loaded for Windows to draw with (wasabiText.ts), once;
  // its face as Winamp named it, "" when it can't be
  _windowsFace: Promise<string> | null = null;
  windowsFace(): Promise<string> {
    if (!this._windowsFace) {
      this._windowsFace = (async () => {
        if (!this._imageManager || !this._file) return "";
        try {
          const url = await this._imageManager.getUrl(this._file);
          if (!url) return "";
          return await skinFace(await (await fetch(url)).arrayBuffer());
        } catch {
          return "";
        }
      })();
    }
    return this._windowsFace;
  }

  // Ensure we've loaded the font into our asset loader.
  async ensureFontLoaded(imageManager: ImageManager) {
    if (!this._file) return;
    Utils.assert(
      this._fontFace == null,
      "Tried to ensure a TrueTypeFont was laoded more than once."
    );
    this._imageManager = imageManager;
    const fontUrl = await imageManager.getUrl(this._file);

    // (a name of letters, digits and dashes only)
    const fontFamily = `font-${Utils.getId()}-${this.getId()}-${this._file}`.replace(/[^A-Za-z0-9_-]+/g, "_");

    // Spotiamp+: a font file the skin doesn't have (Winamp took it from
    // Windows' fonts), or one that won't load: Windows' font of that name,
    // never a skin that fails for it
    if (!fontUrl) {
      this._imageManager = null;
      this._inlineFamily = windowsFontFamily(this._file);
      return;
    }
    try {
      const font = new FontFace(fontFamily, `url(${fontUrl})`);
      this._fontFace = await font.load();
      (document.fonts as any).add(this._fontFace);
    } catch {
      this._imageManager = null;
      this._inlineFamily = windowsFontFamily(this._file);
    }
  }
}
