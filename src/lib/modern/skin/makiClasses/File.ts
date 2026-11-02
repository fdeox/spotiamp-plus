// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import BaseObject from "./BaseObject";
import { UIRoot } from "../../UIRoot";
import { unimplemented } from "../../utils";

export default class File extends BaseObject {
  static GUID = "836f8b2e4db4e0d10a0d7f93d1dcc804";
  _uiRoot: UIRoot;
  _path: string;

  constructor(uiRoot: UIRoot) {
    super();
    this._uiRoot = uiRoot;
  }

  load(path: string) {
    this._path = path;
  }

  // Spotiamp+: nothing is asked of the network (a skin's script looking for
  // Winamp's files made the whole skin fail). A file is there when the skin
  // has it, or when it's one of the plugins of a whole Winamp 5, whose parts
  // Spotiamp+ stands in for (the media library, the visualizers...).
  _entry(): any {
    const path = String(this._path ?? "").replace(/\\/g, "/").replace(/^\.\//, "");
    if (!path) return null;
    try {
      return this._uiRoot._fileExtractor?._find?.(path) ?? null;
    } catch {
      return null;
    }
  }

  exists(): boolean {
    if (this._entry()) return true;
    return WINAMP_PLUGINS.test(String(this._path ?? "").replace(/\\/g, "/"));
  }

  getsize(): number {
    return Number(this._entry()?._data?.uncompressedSize ?? 0);
  }
}

/** The plugins every Winamp 5 install had (Spotiamp+ answers for them). */
const WINAMP_PLUGINS =
  /(^|\/)plugins\/(gen_ml|gen_ff|gen_hotkeys|gen_tray|vis_milk2|vis_avs|vis_nsfs|ml_local|ml_playlists|ml_history|ml_rg|ml_bookmarks|in_mp3|in_vorbis|in_flac|in_mp4|in_wm|in_wave|in_cdda|out_ds|out_wave)\.dll$/i;
