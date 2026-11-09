// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { Emitter, integerToTime, num, toBool } from "../../utils";
import { Track } from "../AudioPlayer";
import { UIRoot } from "../../UIRoot";
import ConfigAttribute from "./ConfigAttribute";

/** What the skin's playlist asks of Spotiamp+'s playlist (set by host.ts). */
export type PlaylistCommands = {
  /** play the row at this index */
  play(index: number): void;
  remove(index: number): void;
  /** move the row at `from` to `to` */
  move(from: number, to: number): void;
  shuffle(on: boolean): void;
  /** 0 off, 1 all, -1 the song */
  repeat(mode: number): void;
};

const nothing = () => {};

/**
 * Non GUI element.
 * Spotiamp+: a mirror of Spotiamp+'s own playlist (rows and the playing one
 * come in from host.ts); playing, removing and moving rows, shuffle and
 * repeat go out to it. Webamp kept its own tracks and opened files here.
 */
export class PlEdit {
  static GUID = "345beebc49210229b66cbe90d9799aa4";
  // taken from lib/pldir.mi
  static guid = "{345BEEBC-0229-4921-90BE-6CB6A49A79D9}";
  _uiRoot: UIRoot;
  _tracks: Track[] = [];
  _currentIndex: number = -1;
  _selection: number[] = [];
  _shuffleAttrib: ConfigAttribute;
  _repeatAttrib: ConfigAttribute;
  _shuffle: boolean;
  _repeat: number = 0; // 0=off | 1=all | -1=track
  _eventListener: Emitter = new Emitter();
  /** set while Spotiamp+'s own shuffle/repeat are written in, so they
   *  aren't sent straight back */
  _mirroring: boolean = false;
  commands: PlaylistCommands = {
    play: nothing,
    remove: nothing,
    move: nothing,
    shuffle: nothing,
    repeat: nothing,
  };

  constructor(uiRoot: UIRoot) {
    this._uiRoot = uiRoot;
    this._listenShuffleRepeat();
  }

  init() {
    this._shuffleChanged(); //trigger to get value from cache storage
    this._repeatChanged(); //trigger to get value from cache storage
  }

  // shortcut of this.Emitter
  on(event: string, callback: Function): Function {
    return this._eventListener.on(event, callback);
  }
  trigger(event: string, ...args: any[]) {
    this._eventListener.trigger(event, ...args);
  }
  off(event: string, callback: Function) {
    this._eventListener.off(event, callback);
  }

  // ---- in, from Spotiamp+ (host.ts) ----

  /** The rows as Spotiamp+'s playlist has them, and which one plays. */
  _setRows(rows: { title: string; durationMs: number }[], current: number) {
    this._tracks = rows.map((row, i) => ({
      id: i + 1,
      filename: row.title,
      title: row.title,
      duration: Math.round((row.durationMs || 0) / 1000),
    }));
    this._currentIndex = current;
    this._selection = this._selection.filter((i) => i < this._tracks.length);
    this.trigger("trackchange");
    this._uiRoot.vm.dispatch(this, "onpleditmodified", []);
  }
  /** Spotiamp+'s shuffle and repeat, shown on the skin's buttons. */
  _setShuffleRepeat(shuffle: boolean, repeat: number) {
    this._mirroring = true;
    try {
      if (this._shuffleAttrib && toBool(this._shuffleAttrib.getdata()) !== shuffle) {
        this._shuffleAttrib.setdata(shuffle ? "1" : "0");
      }
      if (this._repeatAttrib && num(this._repeatAttrib.getdata()) !== repeat) {
        this._repeatAttrib.setdata(String(repeat));
      }
    } finally {
      this._mirroring = false;
    }
  }

  //? ======= shuffle & Repeat Changes =======
  _listenShuffleRepeat() {
    const guid = "{45F3F7C1-A6F3-4EE6-A15E-125E92FC3F8D}"; // pl
    const configItem = this._uiRoot.CONFIG.getitem(guid);
    this._shuffleAttrib = configItem.getattribute("shuffle");
    this._repeatAttrib = configItem.getattribute("repeat");
    // (Spotiamp+: Winamp's own settings always have a value: off until the
    // player says; a button showing them found none and drew nothing)
    if (this._shuffleAttrib.getdata() === "") configItem.setValue("shuffle", "0");
    if (this._repeatAttrib.getdata() === "") configItem.setValue("repeat", "0");
    this._shuffleAttrib.on("datachanged", this._shuffleChanged);
    this._repeatAttrib.on("datachanged", this._repeatChanged);
  }
  _shuffleChanged = () => {
    this._shuffle = toBool(this._shuffleAttrib.getdata());
    if (!this._mirroring) this.commands.shuffle(this._shuffle);
  };
  _repeatChanged = () => {
    this._repeat = num(this._repeatAttrib.getdata());
    if (!this._mirroring) this.commands.repeat(this._repeat);
  };

  //? ======= General PlEdit Information =======
  getnumtracks(): number {
    return this._tracks.length;
  }

  getcurrentindex(): number {
    return this._currentIndex;
  }

  getnumselectedtracks(): number {
    return this._selection.length;
  }

  getnextselectedtrack(i: number): number {
    const current = this._selection.indexOf(i);
    const next = this._selection[current + 1];
    return next ?? -1;
  }

  //? ======= Manipulate PlEdit View =======
  // Scrolls the PL to the currently playling
  // item (mostly used with onKeyDown: space)
  showcurrentlyplayingtrack(): void {
    this.trigger("showtrack", this._currentIndex);
  }

  showtrack(item: number): void {
    this.trigger("showtrack", item);
  }

  /** Webamp added files here; Spotiamp+'s playlist takes them instead. */
  addTrack(track: Track) {}

  enqueuefile(file: string): void {}

  /** A skin's script can't empty the user's playlist; only the list here. */
  clear(): void {
    this._selection = [];
  }

  removetrack(item: number): void {
    if (item >= 0 && item < this._tracks.length) this.commands.remove(item);
  }

  swaptracks(item1: number, item2: number): void {
    this.moveto(item1, item2);
  }

  moveup(item: number): void {
    if (item > 0) this.commands.move(item, item - 1);
  }

  movedown(item: number): void {
    if (item >= 0 && item < this._tracks.length - 1) this.commands.move(item, item + 1);
  }

  moveto(item: number, pos: number): void {
    if (item >= 0 && pos >= 0 && item < this._tracks.length && pos < this._tracks.length && item !== pos) {
      this.commands.move(item, pos);
    }
  }

  currentTrack(): Track | null {
    if (this._currentIndex < 0) {
      return null;
    }
    return this._tracks[this._currentIndex] ?? null;
  }

  playtrack(item: number): void {
    if (item >= 0 && item < this._tracks.length) this.commands.play(item);
  }

  getCurrentTrackTitle(): string {
    if (this._currentIndex < 0 || this._currentIndex >= this._tracks.length) {
      // Spotiamp+: nothing to play: the app and its version, where Winamp 5
      // put its own (the classic face says the same)
      if (this._uiRoot.audio.getState() === "stopped") return this._uiRoot._idleTitle;
      return this._uiRoot._songTitle || this._uiRoot._idleTitle;
    }
    return this.gettitle(this._currentIndex);
  }

  getrating(item: number): number {
    return this._tracks[item]?.rating ?? 0;
  }

  setrating(item: number, rating: number): void {
    if (this._tracks[item]) this._tracks[item].rating = rating;
  }

  gettitle(item: number): string {
    return this._tracks[item]?.title ?? "";
  }

  getlength(item: number): string {
    const track = this._tracks[item];
    return track?.duration ? integerToTime(track.duration) : "";
  }

  getmetadata(item: number, metadatastring: string): string {
    return "";
  }

  getfilename(item: number): string {
    return this._tracks[item]?.filename ?? "";
  }

  onpleditmodified(): void {}
}

/**
 * The PlaylistDirectory object is simply a list with all the saved playlist from the media library.
 * Please remember that this object is always on top of other objects,
 * so you'll have to hide it via maki if you dont want it to be visible.
 * This object was introduced in Winamp 5.5(skinversion 1.3)
 */
// http://wiki.winamp.com/wiki/XML_GUI_Objects#.3CPlaylistDirectory.2F.3E
export class PlDir {
  static GUID = "61a7abad41f67d7980e1d0b1f4a40386";
  // taken from lib/pldir.mi

  showcurrentlyplayingentry(): void {}

  getnumitems(): number {
    return 0;
  }

  getitemname(item: number): string {
    return "";
  }

  refresh(): void {}

  renameitem(item: number, name: string): void {}

  enqueueitem(item: number): void {}

  playitem(item: number): void {}
}
