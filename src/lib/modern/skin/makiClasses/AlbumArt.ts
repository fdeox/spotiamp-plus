// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { UIRoot } from "../../UIRoot";
import AUDIO_PLAYER, { Track } from "../AudioPlayer";
import Layer from "./Layer";

// http://wiki.winamp.com/wiki/XML_GUI_Objects#.3CAlbumArt.2F.3E
export default class AlbumArt extends Layer {
  static GUID = "6dcb05e448c28ac4f04993b14af50e91";
  _trackId: number = -1; // for loading image
  _hasPicture: boolean = false;

  constructor(uiRoot: UIRoot) {
    super(uiRoot);
    this._w = 0;
    this._h = 0;
    this._relatw = "1";
    this._relath = "1";
  }

  init(): void {
    super.init();

    this._uiRoot.playlist.on("trackchange", this._trackChanged);
  }

  // Spotiamp+: the cover of what plays, as the player knows it (host.ts sets
  // audio._albumArtUrl); Webamp read it out of the file's tags
  _trackChanged = () => {
    const url = this._uiRoot.audio._albumArtUrl;
    this._hasPicture = url != null;
    if (url != null) {
      this._div.style.backgroundImage = `url("${encodeURI(url)}")`;
      this._div.style.backgroundSize = "cover";
    } else {
      this._div.style.removeProperty("background-image");
    }
    this._uiRoot.vm.dispatch(this, "onalbumartloaded", [{ type: "BOOLEAN", value: this._hasPicture ? 1 : 0 }]);
  };

  draw() {
    super.draw();
    // this.refresh();
  }

  refresh() {
    const albumArtUrl = this._uiRoot.audio._albumArtUrl;
    if (albumArtUrl != null) {
      this._div.style.pointerEvents = "all";
      this._div.style.backgroundImage = `url(${albumArtUrl})`;
      this._div.style.backgroundSize = "cover";
    } else {
      this._div.style.removeProperty("background-image");
    }
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
