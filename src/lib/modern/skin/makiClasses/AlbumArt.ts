// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { UIRoot } from "../../UIRoot";
import AUDIO_PLAYER, { Track } from "../AudioPlayer";
import Layer from "./Layer";

// http://wiki.winamp.com/wiki/XML_GUI_Objects#.3CAlbumArt.2F.3E
export default class AlbumArt extends Layer {
  static GUID = "6dcb05e448c28ac4f04993b14af50e91";
  _trackId: number = -1; // for loading image
  _hasPicture: boolean = false;
  // Spotiamp+: what it shows with no cover, as Winamp's
  _notFoundImage: string = "winamp.cover.notfound";

  constructor(uiRoot: UIRoot) {
    super(uiRoot);
    this._w = 0;
    this._h = 0;
    this._relatw = "1";
    this._relath = "1";
  }

  setXmlAttr(_key: string, value: string): boolean {
    const key = _key.toLowerCase();
    if (key === "notfoundimage") {
      this._notFoundImage = value;
      if (!this._hasPicture) this._showPicture(null);
      return true;
    }
    return super.setXmlAttr(_key, value);
  }

  init(): void {
    super.init();

    this._uiRoot.playlist.on("trackchange", this._trackChanged);
    if (!this._hasPicture) this._showPicture(this._uiRoot.audio._albumArtUrl);
  }

  // the cover, or its notfoundImage; either keeps its shape, fitted and
  // centred (Winamp's layer_adjustDest)
  _showPicture(url: string | null) {
    const s = this._div.style;
    if (url != null) {
      s.backgroundImage = `url("${encodeURI(url)}")`;
    } else {
      const bitmap = this._uiRoot.getBitmap(this._notFoundImage);
      if (bitmap) s.backgroundImage = `var(${bitmap.getCSSVar()})`;
      else s.removeProperty("background-image");
    }
    s.backgroundSize = "contain";
    s.backgroundPosition = "center";
    s.backgroundRepeat = "no-repeat";
  }

  // Spotiamp+: the cover of what plays, as the player knows it (host.ts sets
  // audio._albumArtUrl); Webamp read it out of the file's tags
  _trackChanged = () => {
    const url = this._uiRoot.audio._albumArtUrl;
    this._hasPicture = url != null;
    this._showPicture(url);
    this._uiRoot.vm.dispatch(this, "onalbumartloaded", [{ type: "BOOLEAN", value: this._hasPicture ? 1 : 0 }]);
  };

  draw() {
    super.draw();
    // this.refresh();
  }

  refresh() {
    const albumArtUrl = this._uiRoot.audio._albumArtUrl;
    if (albumArtUrl != null) this._div.style.pointerEvents = "all";
    this._showPicture(albumArtUrl);
  }

  isloading(): number {
    return 1;
  }

  onAlbumArtLoaded(success: boolean) {
    return true; //TODO
  }

  isinvalid(): boolean {
    return !this._hasPicture;
  }
}
