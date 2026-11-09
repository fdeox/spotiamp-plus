// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import GuiObj from "./GuiObj";
import { UIRoot } from "../../UIRoot";
import TrueTypeFont, { windowsFontFamily } from "../TrueTypeFont";
import BitmapFont from "../BitmapFont";
import {
  colorRgb,
  cssColor,
  integerToTime,
  removeAllChildNodes,
  num,
  px,
  toBool,
  clamp,
} from "../../utils";
import Timer from "./Timer";
import { paintText, systemFace, wasabiTextAvailable } from "../wasabiText";

// http://wiki.winamp.com/wiki/XML_GUI_Objects#.3Ctext.2F.3E_.26_.3CWasabi:Text.2F.3E
export default class Text extends GuiObj {
  static GUID = "efaa867241fa310ea985dcb74bcb5b52";
  _display: string;
  _displayValue: string = "";
  _displayHandler: DisplayHandler; //pasive handler
  _disposeDisplaySubscription: Function;
  _disposeTrackChangedSubscription: Function;
  _text: string;
  _alternateText: string;
  _bold: boolean;
  _forceuppercase: boolean;
  _forcelowercase: boolean;
  _align: string = "left";
  _valign: string = "center";
  _font_id: string;
  _font_obj: TrueTypeFont | BitmapFont;
  _fontSize: number;
  // Spotiamp+: the skin's own number (Wasabi's default is 14); _fontSize is
  // the pixels it's drawn at
  _fontSizeSkin: number = 14;
  _lpadding: number = 0;
  _rpadding: number = 0;
  _color: string;
  _ticker: string = "off"; // "scroll" | "bounce" | "off"
  _paddingX: number = 2;
  _timeColonWidth: number | null = null;
  _timeroffstyle: number = 0;
  _textWrapper: HTMLElement;
  // Spotiamp+: smoothed (Wasabi's default) or hard-edged
  _antialias: boolean = true;
  _scrollTimer: Timer;
  _scrollDirection: -1 | 1;
  _scrollPaused: boolean = false;
  _scrollLeft: number = 0; // logically, not visually
  _textFullWidth: number; //calculated, not runtime by css
  _shadowColor: string;
  _shadowX: number = 0;
  _shadowY: number = 0;
  _drawn: boolean = false; // needed to check has parents

  constructor(uiRoot: UIRoot) {
    super(uiRoot);
    this._uiRoot = uiRoot;
    this._textWrapper = document.createElement("wrap");
    this._div.appendChild(this._textWrapper);
    ensureTextSmoothing();
    this._div.dataset.aa = "1";
    // Spotiamp+: its colours follow the colour theme, as Wasabi's text's
    this._disposeThemeSubscription = uiRoot.on("colorthemechanged", () => this._applyColors());
  }

  _disposeThemeSubscription: Function;

  // Spotiamp+: Winamp's text drawn by Windows (wasabiText.ts) when the app
  // can and the skin is one Winamp drew that way: the canvas it's shown in
  _gdiCanvas: HTMLCanvasElement | null = null;
  _gdiQueued: boolean = false;
  _gdiSeq: number = 0;
  _gdiResize: ResizeObserver | null = null;
  _offsetX: number = 0;
  _offsetY: number = 0;
  _forceFixed: boolean = false;
  _wrapped: boolean = false;
  _timerHours: boolean = false;
  // Winamp's suggested width for the text last painted, and the one a script
  // was last told (getautowidth): a script laying out by it hears again when
  // Windows' measure differs from the browser's
  _gdiAuto: { text: string; w: number } | null = null;
  _autoWidthTold: number | null = null;

  _printedText(): string {
    const text = this.gettext() ?? "";
    if (this._forceuppercase) return text.toUpperCase();
    if (this._forcelowercase) return text.toLowerCase();
    return text;
  }

  _gdiOn(): boolean {
    if (this._font_obj instanceof BitmapFont) return false;
    return wasabiTextAvailable();
  }

  // Winamp drew 1.3 and later skins' text with Windows' GDI, older ones with
  // its FreeType renderer
  _renderer(): "gdi" | "freetype" {
    return (this._uiRoot._skinVersion ?? 0) >= 1.3 ? "gdi" : "freetype";
  }

  // FreeType's font, as that renderer found it: the skin's own file (and the
  // name it knew it by: the file's, up to its first dot), else Windows'
  _ftFont(face: string): { ft_font: string; ft_name: string } {
    const font = this._font_obj;
    if (font instanceof TrueTypeFont && font.hasUrl() && face) {
      const name = String(font._file ?? "").split(".")[0].split(/[\/]/).pop() ?? "";
      return { ft_font: `face:${face}`, ft_name: name };
    }
    return { ft_font: `sys:${this._font_id || "Arial"}`, ft_name: this._font_id || "Arial" };
  }

  _gdiSchedule() {
    if (this._gdiQueued || !this._drawn || !this._gdiOn()) return;
    this._gdiQueued = true;
    setTimeout(() => this._gdiPaint(), 0);
  }

  // the face Windows draws it with, as Winamp found the font
  async _gdiFace(): Promise<string> {
    const font = this._font_obj;
    if (font instanceof TrueTypeFont && font.hasUrl()) {
      const face = await font.windowsFace();
      if (face) return face;
    }
    // none of the skin's: Windows' fonts folder by that name, else Arial
    return systemFace(this._font_id || "Arial");
  }

  _gdiColor(): [number, number, number] {
    if (this._color && !colorRgb(this._color)) {
      return this._uiRoot._namedColorRgb(this._color) ?? [255, 255, 255];
    }
    return this._uiRoot._getGammaGroup("Text").transformColor2rgb(this._color || "255,255,255");
  }

  _gdiShadowColor(): [number, number, number] {
    if (this._shadowColor && !colorRgb(this._shadowColor)) {
      return this._uiRoot._namedColorRgb(this._shadowColor) ?? [0, 0, 0];
    }
    return this._uiRoot._getGammaGroup("Text backgrounds").transformColor2rgb(this._shadowColor || "0,0,0");
  }

  _isTimeDisplay(): boolean {
    const d = (this._display ?? "").toLowerCase();
    return d === "time" || d === "timeelapsed" || d === "timeremaining";
  }

  async _gdiPaint() {
    this._gdiQueued = false;
    if (!this._gdiOn()) return;
    const seq = ++this._gdiSeq;
    if (!this._gdiResize && typeof ResizeObserver !== "undefined") {
      this._gdiResize = new ResizeObserver(() => this._gdiSchedule());
      this._gdiResize.observe(this._div);
    }
    const w = this._div.offsetWidth;
    const h = this._div.offsetHeight;
    const text = this._printedText();
    const face = await this._gdiFace();
    if (seq !== this._gdiSeq) return;
    if (!w || !h) {
      this._gdiShow(null);
      return;
    }
    const painted = await paintText({
      face,
      size: this._fontSizeSkin,
      bold: this._bold ? 1 : 0,
      antialias: this._antialias,
      text,
      w,
      h,
      align: { left: 0, center: 1, right: 2 }[String(this._align).toLowerCase()] ?? 0,
      valign: { top: 0, bottom: 2 }[String(this._valign).toLowerCase()] ?? 1,
      lpadding: this._lpadding,
      rpadding: this._rpadding,
      offsetx: this._offsetX,
      offsety: this._offsetY,
      color: this._gdiColor(),
      shadow: { x: this._shadowX || 0, y: this._shadowY || 0, color: this._gdiShadowColor() },
      wrapped: this._wrapped,
      time: this._isTimeDisplay()
        ? { hours: this._timerHours, fixed: this._forceFixed, colon: this._timeColonWidth ?? -1 }
        : null,
      renderer: this._renderer(),
      ...this._ftFont(face),
    });
    if (seq !== this._gdiSeq) return;
    this._gdiShow(painted);
    if (painted) {
      this._gdiAuto = { text, w: painted.suggested };
      if (this._autoWidthTold != null && this._autoWidthTold !== painted.suggested) {
        this._autoWidthTold = null;
        this._uiRoot.vm.dispatch(this, "ontextchanged", [{ type: "STRING", value: this.gettext() }]);
      }
    }
  }

  // the painted text at the box's corner (a ticker moves it), the browser's
  // hidden; nothing painted: the browser's back
  _gdiShow(painted: { w: number; h: number; textW: number; suggested: number; data: Uint8ClampedArray } | null) {
    if (!painted) {
      delete this._div.dataset.gdi;
      this._gdiCanvas?.remove();
      this._gdiCanvas = null;
      if (!this._wGiven) this._div.style.removeProperty("width");
      return;
    }
    if (!this._gdiCanvas) {
      this._gdiCanvas = document.createElement("canvas");
      this._gdiCanvas.className = "gdi-text";
      this._div.appendChild(this._gdiCanvas);
    }
    const canvas = this._gdiCanvas;
    if (canvas.width !== painted.w) canvas.width = painted.w;
    if (canvas.height !== painted.h) canvas.height = painted.h;
    canvas.getContext("2d").putImageData(new ImageData(painted.data, painted.w, painted.h), 0, 0);
    canvas.style.left = this._textWrapper.style.left || "0px";
    this._div.dataset.gdi = "1";
    if (!this._wGiven) this._div.style.width = px(painted.suggested);
    if (this._ticker != "off") {
      this._textFullWidth = painted.w;
      // (cur_len: the text and, when it fit them, its paddings)
      const pads = (this._lpadding ?? 0) + (this._rpadding ?? 0);
      this._tickLen = painted.textW + (painted.textW <= painted.w - 2 - pads ? pads : 0);
      this._div.style.setProperty("--full-width", px(painted.w));
    }
  }

  // Spotiamp+: the skin (or a script) gave it a height; else it's its own,
  // as Wasabi's group sizes an object given none
  _hGiven: boolean = false;
  // ...and a width (else Winamp's suggested one, once Windows has measured)
  _wGiven: boolean = false;

  resize(x: number, y: number, w: number, h: number) {
    this._hGiven = true;
    this._wGiven = true;
    super.resize(x, y, w, h);
  }

  getheight(): number {
    return this._hGiven ? super.getheight() : this.getautoheight();
  }

  // as Wasabi's text: its font size (the skin's number) times its lines,
  // 32 with no text
  getautoheight(): number {
    const text = this.gettext();
    return text ? this._fontSizeSkin * String(text).split("\n").length : 32;
  }

  _renderHeight() {
    if (this._hGiven) return super._renderHeight();
    this._div.style.height = px(this.getautoheight());
  }

  // Spotiamp+: the text's and its shadow's colours in the colour theme on
  _applyColors() {
    this._gdiSchedule();
    if (!(this._font_obj instanceof BitmapFont)) this._autoDetectColor();
    if (this._shadowColor) {
      this._div.style.textShadow = `${this._shadowX}px ${this._shadowY}px ${this._shadowCss()}`;
    }
  }

  // Spotiamp+: as Wasabi's text: a skin colour as the skin has it, else
  // its own (black when none) in the "Text backgrounds" group
  _shadowCss(): string {
    const named = !colorRgb(this._shadowColor) && this._uiRoot.getColor(this._shadowColor);
    if (named) return `var(${named.getCSSVar()}, ${named.getRgb()})`;
    const own = colorRgb(this._shadowColor) ? this._shadowColor : "0,0,0";
    return this._uiRoot._getGammaGroup("Text backgrounds").transformColor(own);
  }

  setXmlAttr(key: string, value: string): boolean {
    const lower = key.toLowerCase();
    if (lower === "h" || lower === "default_h") this._hGiven = true;
    if (lower === "w" || lower === "default_w") this._wGiven = true;
    if (super.setXmlAttr(key, value)) {
      return true;
    }
    switch (lower) {
      case "display":
        // (str) Either a specific system display string or the string identifier of a text feed. Setting this value will override the text parameter. See below.
        this._setDisplay(value);
        break;
      case "text":
      case "default":
        // (str) A static string to be displayed.
        // console.log('THETEXT', value)
        // if (value.startsWith(':')) {
        //   value = this._interpolateText(value)
        // }
        this._text = value;
        this._renderText();
        break;
      case "bold":
        // (str) A static string to be displayed.
        this._bold = toBool(value);
        this._prepareCss();
        break;
      case "antialias":
        // Spotiamp+: (bool) smoothed or hard-edged, as Wasabi asks Windows
        this._antialias = toBool(value);
        this._div.dataset.aa = this._antialias ? "1" : "0";
        this._gdiSchedule();
        break;
      case "showlen":
        // (Spotiamp+: the song's length after its title, as Wasabi's text
        // added it: " (m:ss)")
        this._showlen = toBool(value);
        if (this._display && /^song(name|title)$/i.test(this._display)) this.setDisplayValue(this._songTitleText());
        break;
      case "forceupcase":
      case "forceuppercase":
        // (bool) Force the system to make the display string all uppercase before display.
        this._forceuppercase = toBool(value);
        this._prepareCss();
        this._renderText();
        break;
      case "font":
        // (id) The id of a bitmapfont or truetypefont element. If no element with that id can be found, the OS will be asked for a font with that name instead.
        this._font_id = value;
        this._autoDetectFontType();
        this.ensureFontSize();
        this._prepareCss();
        break;
      case "align":
        // (str) One of the following three possible strings: "left" "center" "right" -- Default is "left."
        this._align = value;
        this._prepareCss();
        break;

      case "valign":
        // (str) One of the following three possible strings: "top" "center" "bottom" -- Default is "center."
        this._valign = value;
        this._prepareCss();
        break;

      case "fontsize": {
        // (int) The size to render the chosen font.
        // Spotiamp+: as Wasabi, "+2", "-2", "*2" and "/2" change the size it has
        const n = num(value.replace(/^[-+*/]/, "")) ?? 0;
        const op = value.trim()[0];
        const cur = this._fontSizeSkin;
        this._fontSizeSkin =
          op == "+" ? cur + n : op == "-" ? cur - n : op == "*" ? cur * n : op == "/" ? (n ? Math.trunc(cur / n) : cur) : (num(value) ?? 14);
        this._fontSize = this._fontSizeSkin;
        this.ensureFontSize();
        this._invalidateFullWidth();
        this._prepareCss();
        break;
      }
      case "leftpadding":
        this._lpadding = num(value) ?? 0;
        this._gdiSchedule();
        break;
      case "rightpadding":
        this._rpadding = num(value) ?? 0;
        this._gdiSchedule();
        break;
      // Spotiamp+: the rest of Winamp's text's layout, for Windows' drawing
      case "offsetx":
        this._offsetX = num(value) ?? 0;
        this._gdiSchedule();
        break;
      case "offsety":
        this._offsetY = num(value) ?? 0;
        this._gdiSchedule();
        break;
      case "forcefixed":
        this._forceFixed = toBool(value);
        this._gdiSchedule();
        break;
      case "wrap":
      case "wrapped":
        this._wrapped = toBool(value);
        this._gdiSchedule();
        break;
      case "timerhours":
        this._timerHours = toBool(value);
        this._gdiSchedule();
        break;
      case "forcelocase":
      case "forcelowercase":
        this._forcelowercase = toBool(value);
        this._renderText();
        break;
      case "color":
        // (int[sic?]) The comma delimited RGB color of the text.
        this._color = value;
        this._prepareCss();
        break;
      case "ticker":
        /// (bool) Setting this flag causes the object to scroll left and right if the text does not fit the rectangular area of the text object.
        if (value == "0") value = "off";
        this._ticker = value.toLowerCase();
        break;
      case "timecolonwidth":
        // (int) How many extra pixels wider or smaller should the colon be when displaying time. Default is -1.
        this._timeColonWidth = num(value);
        this._prepareCss();
        this._renderText();
        break;
      case "timeroffstyle":
        this._timeroffstyle = num(value);
        this._setDisplay(this._display);
        break;
      case "shadowcolor":
        // (int) The comma delimited RGB color for underrendered shadow text.
        this._shadowColor = value;
        this._prepareCss();
        break;
      case "shadowx":
        // (int) The x offset of the shadowrender.
        this._shadowX = num(value);
        this._prepareCss();
        break;
      case "shadowy":
        // (int) The x offset of the shadowrender.
        this._shadowY = num(value);
        this._prepareCss();
        break;

      /*
antialias - (bool) Setting this flag causes the text to be rendered antialiased if possible.
default - (str) A parameter alias for text.
align - (str) One of the following three possible strings: "left" "center" "right" -- Default is "left."
timeroffstyle - (int) How to display an empty timer: "0" = "  : ", "1" = "00:00", and "2"="" (if one is displaying time)
nograb - (bool) Setting this flag will cause the text object to ignore left button down messages. Default is off.
showlen - (bool) Setting this flag will cause the text display to be appended with the length in minutes and seconds of the current song. Default is off.
forcefixed - (bool) Force the system to attempt to render the display string with fixed-width font spacing.
forcelocase - (bool) Force the system to make the display string all lowercase before display.
forcelowercase - (bool) Force the system to make the display string all lowercase before display.
wrap - (bool) Setting this flag will cause the text to wrap in its rectangular space. Default is off.
dblclickaction - (str) A string in the form "SWITCH;layout" where layout is the id of a layout in this object's parent container. No other actions function on this object. This action is deprecated.
offsetx - (int) Extra pixels to be added to or subtracted from the calculated x value for the display string to render.
offsety - (int) Extra pixels to be added to or subtracted from the calculated x value for the display string to render.
*/
      default:
        return false;
    }
    return true;
  }

  // styledTime(time: string): string {
  //   const originalTime = time;
  //   if (this._timeroffstyle == 1) {
  //     if (time.startsWith("-") && time.length == 5) {
  //       // -9:59
  //       time = time.replace("-", "-0");
  //     } else if (time.length == 4) {
  //       // 9:59
  //       time = "0" + time;
  //     }
  //   }
  //   console.log(`timer:'${time}' < '${originalTime}'`);
  //   return time;
  // }

  _autoDetectFontType() {
    if (this._font_id) {
      this._font_obj = this._uiRoot.getFont(this._font_id);
      if (!this._font_obj) {
        const newFont = new TrueTypeFont();
        // (Spotiamp+: as Winamp, Windows' font of that name, kept under it)
        newFont._id = this._font_id;
        newFont._inlineFamily = windowsFontFamily(this._font_id);
        this._uiRoot.addFont(newFont);
        this._font_obj = newFont;
      }
    }
  }

  // only applicable for TrueType Font
  _autoDetectColor() {
    // Spotiamp+: as Wasabi's TextBase: its own colour (white when none) in
    // the colour theme's "Text" group; a skin colour is in its own (UIRoot)
    if (!this._color || colorRgb(this._color)) {
      this._div.style.color = this._uiRoot._getGammaGroup("Text").transformColor(this._color || "255,255,255");
      return;
    }
    if (this._color) {
      const color = this._uiRoot.getColor(this._color);
      if (color) {
        this._div.style.color = `var(${color.getCSSVar()}, ${color.getRgb()})`;
      }
    }
  }

  // Spotiamp+: as Wasabi (truetypefont_win32): a font of size n is drawn
  // MulDiv(n, 72, 96) pixels high, whatever the font (and Wasabi's own
  // Arial when the skin names none)
  ensureFontSize() {
    this._fontSize =
      this._font_obj instanceof BitmapFont ? this._fontSizeSkin : Math.round((this._fontSizeSkin * 72) / 96);
  }

  init() {
    super.init();
    if (this._ticker && this._ticker != "off") {
      this._prepareScrolling();
    }
    this._div.addEventListener("click", this._onClick);
    if (this._displayHandler != null) {
      this._displayHandler.init();
    }
  }

  _onClick = () => {
    if (this._display.toLowerCase() == "time") {
      this._uiRoot.audio.toggleRemainingTime();
      this.setDisplayTime();
    }
  };

  _setDisplay(display: string) {
    if (display == null) {
      // this method may called premateurly by timeroffstyle, hence display==unset
      return;
    }
    // if (display.toLowerCase() === this._display?.toLowerCase()) {
    //   return;
    // }
    if (this._disposeDisplaySubscription != null) {
      this._disposeDisplaySubscription();
    }
    if (this._disposeTrackChangedSubscription != null) {
      this._disposeTrackChangedSubscription();
    }
    this._display = display;
    switch (this._display.toLowerCase()) {
      case "":
        this._displayValue = "";
        break;
      case "pe_info":
        // Spotiamp+: the playlist's length, as Winamp's playlist shows it
        this._displayValue = this._playlistInfo();
        this._disposeTrackChangedSubscription = this._uiRoot.playlist.on(
          "trackchange",
          () => this.setDisplayValue(this._playlistInfo())
        );
        break;
      case "vid_info":
        // no video in Spotiamp+
        this._displayValue = "";
        break;
      case "time":
      case "timeelapsed":
      case "timeremaining":
        this._disposeDisplaySubscription =
          this._uiRoot.audio.onCurrentTimeChange(() => {
            // this.setDisplayValue(this.styledTime(
            //   integerToTime(this._uiRoot.audio.getCurrentTime())
            // ));
            this.setDisplayTime();
          });
        console.log("in changing display = time. by:", display);
        this.setDisplayTime();
        // this.setDisplayValue(this.styledTime(
        //   integerToTime(this._uiRoot.audio.getCurrentTime())
        // ));
        break;
      case "songlength":
        // Spotiamp+: the song's real length (Webamp had a placeholder)
        this._displayValue = this._songLength();
        this._disposeDisplaySubscription =
          this._uiRoot.audio.onCurrentTimeChange(() =>
            this.setDisplayValue(this._songLength())
          );
        break;
      case "songname":
      case "songtitle":
        this._displayValue = this._songTitleText();
        this._disposeTrackChangedSubscription = this._uiRoot.playlist.on(
          "trackchange",
          () => {
            this._displayValue = this._songTitleText();
            this._renderText();
          }
        );
        break;
      case "songartist":
      case "songalbum": {
        // Spotiamp+: what's playing's artist, its album
        const meta = () => this._uiRoot._songMeta?.[this._display.toLowerCase() === "songartist" ? "artist" : "album"] ?? "";
        this._displayValue = meta();
        this._disposeTrackChangedSubscription = this._uiRoot.playlist.on(
          "trackchange",
          () => this.setDisplayValue(meta())
        );
        break;
      }
      case "songbitrate":
      case "songsamplerate":
      case "songinfo":
        // Spotiamp+: what's playing, as Winamp writes it (host.ts)
        this._displayValue = this._songInfo();
        this._disposeTrackChangedSubscription = this._uiRoot.playlist.on(
          "trackchange",
          () => this.setDisplayValue(this._songInfo())
        );
        break;
      case "componentbucket":
        // Spotiamp+: Wasabi's feed of that name: the component under the
        // mouse in a component bucket, else nothing (ComponentBucket.ts)
        this._displayValue = "";
        this._disposeTrackChangedSubscription = this._uiRoot.on("componentbuckettext", (name) => this.setDisplayValue(name ?? ""));
        break;
      case "custom": // not winamp api
        // needed by a custom DisplayHandler
        break;
      default:
        // a display Spotiamp+ doesn't know: empty, and the skin goes on
        console.warn(`Unknown text display name: "${this._display}".`);
        this._displayValue = "";
    }
    this._renderText();
  }

  // Spotiamp+: as Winamp's playlist shows it: the selected songs' length and
  // the whole list's, "0:00/0:00" when empty
  _playlistInfo(): string {
    const pl = this._uiRoot.playlist;
    let total = 0;
    let selected = 0;
    pl._tracks.forEach((track, i) => {
      total += track.duration || 0;
      if (pl._selection.includes(i)) selected += track.duration || 0;
    });
    const fmt = (s: number) => {
      s = Math.floor(s);
      const h = Math.floor(s / 3600);
      const m = Math.floor((s % 3600) / 60);
      const sec = String(s % 60).padStart(2, "0");
      return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
    };
    return `${fmt(selected)}/${fmt(total)}`;
  }

  // Spotiamp+: showlen: the title, and " (m:ss)" when the song's length is
  // known (Wasabi's Text: core_getTitle and wa2.getLength)
  _showlen: boolean = false;
  _songTitleText(): string {
    const title = this._uiRoot.playlist.getCurrentTrackTitle();
    if (!this._showlen) return title;
    const pl = this._uiRoot.playlist;
    const track = pl.currentTrack?.();
    const sec = Math.floor(this._uiRoot.audio.getLength?.() || track?.duration || 0);
    if (!track || sec <= 0) return title;
    return `${title} (${Math.floor(sec / 60)}:${String(sec % 60).padStart(2, "0")})`;
  }

  _songLength(): string {
    const length = this._uiRoot.audio.getLength();
    return length > 0 ? integerToTime(length) : "";
  }

  _songInfo(): string {
    const { kbps, khz, stereo } = this._uiRoot._songInfo;
    switch (this._display.toLowerCase()) {
      case "songbitrate":
        return kbps ? String(kbps) : "";
      case "songsamplerate":
        return khz ? String(khz) : "";
      default:
        if (!kbps && !khz) return "";
        return `${kbps ? kbps + "kbps " : ""}${stereo ? "stereo" : "mono"} ${khz ? khz + "kHz" : ""}`.trim();
    }
  }

  setDisplayValue(newValue: string) {
    if (newValue !== this._displayValue) {
      this._displayValue = newValue;
      this._renderText();
      this._uiRoot.vm.dispatch(this, "ontextchanged", [
        { type: "STRING", value: this.gettext() },
      ]);
    }
  }

  setDisplayTime() {
    if (this._uiRoot.audio._isStop) {
      switch (this._timeroffstyle) {
        case 0:
          this.setDisplayValue("  : ");
          break;
        case 1:
          this.setDisplayValue("00:00");
          break;
        case 2:
          this.setDisplayValue("");
          break;
      }
      return;
    }
    const audio = this._uiRoot.audio;
    const remaining =
      this._display?.toLowerCase() === "timeremaining" && audio.getLength() > 0;
    this.setDisplayValue(
      remaining
        ? "-" + integerToTime(audio.getLength() - Math.abs(audio._position))
        : integerToTime(audio.getCurrentTime())
    );
  }

  ontextchanged(s: string) {
    this._uiRoot.vm.dispatch(this, "ontextchanged", [
      { type: "STRING", value: this.gettext() },
    ]);
  }

  _interpolateText(value: string): string {
    switch (value.toLowerCase()) {
      case ":componentname":
        const layout = this.getparentlayout();
        if (layout) {
          //debugger
          try {
            // sometime error with wasabi
            return layout.getcontainer()._name || value;
          } catch {
            return value;
          }
        }
        break;
    }
    return value;
  }

  gettext(): string {
    if (this._alternateText) {
      // alternate text is used in Winamp3 to show a hint of a Play button while mouse down.
      return this._alternateText;
    }
    if ((this._text || "").startsWith(":") && this._drawn) {
      const layout = this.getparentlayout();
      if (layout) {
        return layout.getcontainer()._name || this._text;
      }
    }
    if (this._display) {
      if (this._display == "songinfo") {
        const track = this._uiRoot.playlist.currentTrack();
        // (Spotiamp+: its rows carry no file metadata: the player's info then)
        if (track?.metadata) {
          const m = track.metadata;
          return `${m.bitrate}kbps ${m.channelMode} ${Math.floor(
            m.sampleRate / 1000
          )}khz`;
        }
      }

      return this._displayValue;
    }
    return this._text ?? "";
  }

  // extern Text.setText(String txt); // changes the display/text="something" param
  settext(txt: string) {
    if (this._text != txt) {
      this._text = txt;
      this._renderText();
    }
  }
  // overrides the display/text parameter with a custom string, set "" to cancel
  setalternatetext(txt: string) {
    this._alternateText = txt;
    this._renderText();
  }

  //to speedup animation like text-scrolling, we spit rendering processes.
  //This function is only rendering static styles
  _prepareCss() {
    this._gdiSchedule();
    if (!this._font_obj && this._font_id) {
      this._font_obj = this._uiRoot.getFont(this._font_id);
    }
    const font = this._font_obj;
    if (font instanceof BitmapFont) {
      this._textWrapper.setAttribute("font", "BitmapFont");
      this._div.style.removeProperty("background-image");
      //? font-size
      this._div.style.setProperty(
        "--fontSize",
        (this._fontSize || "~").toString()
      );
      //? text-align
      if (this._align != "center") {
        this._div.style.setProperty("--align", this._align);
      } else {
        this._div.style.removeProperty("--align");
      }
      //? vertical align
      if (this._valign != "center") {
        this._div.style.setProperty(
          "--valign",
          this._valign == "top" ? "flex-start" : "flex-end"
        );
      } else {
        this._div.style.removeProperty("--valign");
      }
      //? margin
      this._div.style.setProperty(
        "--hspacing",
        px(font.getHorizontalSpacing())
      );

      this.setBackgroundImage(font);
      this._div.style.backgroundSize = "0"; //disable parent background, because only children will use it
      this._div.style.lineHeight = px(this._div.getBoundingClientRect().height);
      this._div.style.setProperty("--charwidth", px(font._charWidth));
      this._div.style.setProperty("--charheight", px(font._charHeight));
    } else {
      this._autoDetectColor();
      // Spotiamp+: a TrueType text has no picture of its own: not one a
      // group or button around it has (their CSS variables are inherited,
      // hovered and pressed ones too), as Winamp's
      this._div.style.backgroundImage = "none";
      // Spotiamp+: placed in its box as Wasabi does (elements.css)
      const flex = { left: "flex-start", top: "flex-start", right: "flex-end", bottom: "flex-end" };
      this._div.style.setProperty("--talign", flex[this._align] ?? "center");
      this._div.style.setProperty("--tvalign", flex[this._valign] ?? "center");
      if (this._shadowColor) {
        this._div.style.textShadow = `${this._shadowX}px ${this._shadowY}px ${this._shadowCss()}`;
      }
      if (font instanceof TrueTypeFont) {
        this._textWrapper.setAttribute("font", "TrueType");

        this._div.style.fontFamily = font.getFontFamily();
        this._div.style.fontSize = px(this._fontSize ?? 11);
        // this._div.style.lineHeight = px(this._fontSize ?? 11);
        // this._div.style.lineHeight = px(this._div.getBoundingClientRect().height);
        this._div.style.lineHeight = "1";
        this._div.style.textTransform = this._forceuppercase
          ? "uppercase"
          : "none";
        if (this._bold) {
          this._div.style.fontWeight = "bold";
        }
        if (this._align) {
          this._div.style.textAlign = this._align;
        }
      } else if (font == null) {
        this._div.style.setProperty("--fontMode", "Null");
        this._div.style.fontFamily = "Arial";
        this._div.style.fontSize = px(this._fontSize ?? 11);
        this._div.style.fontWeight = this._bold ? "bold" : "";
        this._div.style.textTransform = this._forceuppercase ? "uppercase" : "none";
      } else {
        throw new Error("Unexpected font");
      }
    }
  }

  _renderText() {
    this._gdiSchedule();
    //TODO: invalidating text width is only important when srolling?
    if (this._ticker != "off") this._invalidateFullWidth();
    // (Spotiamp+: a text sized by its own lines follows them)
    if (!this._hGiven) this._renderHeight();

    const font = this._font_obj;
    if (font instanceof BitmapFont) {
      this._renderBitmapFont(font);
    } else {
      // console.log('THETEXT', this.gettext())
      // this._textWrapper.innerHTML = '<pre>'+ this.gettext().replace(/[\n\r]/g,'<br/>') + '</pre>';
      // this._textWrapper.innerHTML = '<div>'+ this.gettext().replace(/[\n]/g,'<br/>') + '</div>';
      // this._textWrapper.innerHTML = '<pre>'+ this.gettext() + '</pre>';
      this._textWrapper.innerText = this.gettext();
      //? workaround of titlebar.text
      // this._div.style.lineHeight = this._div.style.height;
    }
  }

  _useColonWidth() {
    if (this._timeColonWidth == null || this._display == null) {
      return false;
    }
    switch (this._display.toLowerCase()) {
      case "time":
      case "timeelapsed":
      case "timeremaining":
        return true;
    }
    return false;
  }

  _renderBitmapFont(font: BitmapFont) {
    removeAllChildNodes(this._textWrapper);
    this._div.style.whiteSpace = "nowrap";
    const useColonWidth = this._useColonWidth();
    if (this.gettext() != null) {
      for (const char of this.gettext().split("")) {
        const charNode = font.renderLetter(char);
        // TODO: This is quite hacky.
        if (char === ":" && useColonWidth) {
          charNode.style.width = px(this._timeColonWidth);
          charNode.style.marginRight = "0";
        }
        this._textWrapper.appendChild(charNode);
      }
    }
  }

  // it is needed for scrolltext.
  _invalidateFullWidth() {
    const font = this._font_obj;
    if (font instanceof BitmapFont) {
      this._textFullWidth = this._getBitmapFontTextWidth(font);
    } else {
      this._textFullWidth = this._getTrueTypeTextWidth(font);
    }
    this._div.style.setProperty("--full-width", px(this._textFullWidth));
  }

  // Spotiamp+: as Wasabi: the text's own width + 4 and its paddings (32
  // for no text), not its box's
  getautowidth(): number {
    // (Windows' measure when its paint is in, as Winamp's)
    if (this._gdiOn() && this._gdiAuto && this._gdiAuto.text === this._printedText()) {
      return (this._autoWidthTold = this._gdiAuto.w);
    }
    if (!this.gettext()) return (this._autoWidthTold = 32);
    this._invalidateFullWidth();
    return (this._autoWidthTold = this._textFullWidth + 4 + this._lpadding + this._rpadding);
  }

  gettextwidth(): number {
    this._invalidateFullWidth();
    return this._textFullWidth + 4;
  }

  _getBitmapFontTextWidth(font: BitmapFont): number {
    const charWidth = font._charWidth;
    return this.gettext().length * charWidth;
  }

  _getTrueTypeTextWidth(font: TrueTypeFont): number {
    /**
     * Uses canvas.measureText to compute and return the width of the given text of given font in pixels.
     *
     * @param {String} text The text to be rendered.
     * @param {String} font The css font descriptor that text is to be rendered with (e.g. "bold 14px verdana").
     *
     * @see https://stackoverflow.com/questions/118241/calculate-text-width-with-javascript/21015393#21015393
     */
    // const self = this;
    let txt = this.gettext();
    if (this._forceuppercase) {
      txt = txt.toUpperCase();
    } else if (this._forcelowercase) {
      txt = txt.toLowerCase();
    }
    const fontFamily =
      (font && font.getFontFamily()) ||
      '"Liberation Sans", "DejaVu Sans", Arial';

    const canvas = document.createElement("canvas");
    const context = canvas.getContext("2d");
    context.font = `${this._bold ? "700" : ""} ${
      this._fontSize || 11
    }px ${fontFamily}`;

    // (Spotiamp+: the text as it shows, upper or lower case)
    const metrics = context.measureText(txt);

    return Math.ceil(metrics.width /*+ self._paddingX * 2*/);
  }

  draw() {
    this._drawn = true;
    super.draw();
    this._renderText();
    this._gdiSchedule();

    // this._div.style.removeProperty("line-height");
    this._div.classList.add("webamp--img");
  }

  // Spotiamp+: Wasabi's ticker (text.cpp, advanceTicker): only a song's
  // texts, the time and a component bucket's move: a pixel every second
  // tick (Winamp's "Text Ticker Speed", a half) of 25 ms (250 for the time,
  // 50 for a bucket), there and back, waiting 4 s at each end and at the
  // start of each new text; any other text stays put
  _tickPos: number = 0;
  _tickSens: number = 0;
  _tickTts: number = 20;
  _tickTimeTts: number = 20;
  _tickSkip: number = 0;
  _tickText: string | null = null;
  /** the text's width as Wasabi measured it (cur_len), Windows' when painted */
  _tickLen: number = 0;

  _tickerPeriod(): number {
    const d = String(this._display ?? "").toLowerCase();
    if (/^song(name|title|artist|album|length|info|info_translated)$/.test(d)) return 25;
    if (d === "time") return 250;
    if (d.startsWith("componentbucket")) return 50;
    return 0;
  }

  _prepareScrolling() {
    const period = this._tickerPeriod();
    if (!period) return;
    this._tickTimeTts = Math.floor(4000 / period);
    this._tickTts = this._tickTimeTts;
    const timer = (this._scrollTimer = new Timer(this._uiRoot));
    timer.setdelay(period);
    timer.setOnTimer(() => {
      this.doScrollText();
    });
    timer.start();
  }

  doScrollText() {
    const text = this._printedText();
    if (text !== this._tickText) {
      // (resetTicker: a new text from its start, after the wait)
      this._tickText = text;
      this._tickSens = 0;
      this._tickPos = 0;
      this._tickTts = this._tickTimeTts;
    }
    const w = this._div.offsetWidth;
    const len = this._gdiCanvas ? this._tickLen : this._textFullWidth;
    const room = w - 2;
    if (len < room) this._tickPos = 0;
    else if (this._tickTts > 0) this._tickTts--;
    else if (this._tickSkip < 1) this._tickSkip++;
    else {
      this._tickSkip = 0;
      this._tickPos += this._tickSens ? -1 : 1;
      if (this._tickPos < 0) this._tickPos = 0;
      if (this._tickPos > len - room) this._tickPos = len - room;
      if (len <= this._tickPos + room) {
        this._tickSens = 1;
        this._tickTts = this._tickTimeTts;
      }
      if (this._tickPos <= 0) {
        this._tickSens = 0;
        this._tickPos = 0;
        this._tickTts = this._tickTimeTts;
      }
    }
    const left = px(-Math.round(this._tickPos));
    if (this._textWrapper.style.left !== left) this._textWrapper.style.left = left;
    if (this._gdiCanvas && this._gdiCanvas.style.left !== left) this._gdiCanvas.style.left = left;
  }

  dispose() {
    this._disposeThemeSubscription?.();
    this._gdiResize?.disconnect();
    this._gdiSeq++;
    if (this._disposeDisplaySubscription != null) {
      this._disposeDisplaySubscription();
    }
    if (this._displayHandler != null) {
      this._displayHandler.dispose();
    }
  }

  /*
  
extern String Text.getText();
extern int Text.getTextWidth();
extern Text.onTextChanged(String newtxt);
  */

  /**
   *
   * @param Handler a class inherited from DisplayHandler
   */
  setDisplayHandler(Handler: DisplayHandlerClass) {
    if (this._displayHandler != null) {
      this._displayHandler.dispose();
    }
    this._displayHandler = new Handler(this);
  }
}

type DisplayHandlerClass = typeof DisplayHandler;

export class DisplayHandler {
  _text: Text;
  _uiRoot: UIRoot;
  _subscription: Function;

  constructor(text: Text) {
    this._text = text;
    this._uiRoot = text._uiRoot;
    this._subscription = () => {}; // deFault empty
  }

  init(): void {
    //* possible add a hook to uiRoot
    //* to update text, call:
    //*   `this._text.setDisplayValue(newTextValue)`
  }

  dispose(): void {
    this._subscription();
  }
}

// Spotiamp+: the filters text is smoothed with as Wasabi asks Windows to
// (elements.css): grey antialiasing (an empty filter, which keeps Chromium
// from ClearType's coloured edges) and none (the edge's alpha cut at half)
let textSmoothingReady = false;
function ensureTextSmoothing() {
  if (textSmoothingReady || typeof document === "undefined" || !document.body) return;
  textSmoothingReady = true;
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("width", "0");
  svg.setAttribute("height", "0");
  svg.setAttribute("aria-hidden", "true");
  svg.style.position = "absolute";
  svg.innerHTML =
    '<filter id="wasabi-text-grey"><feComponentTransfer><feFuncA type="identity"/></feComponentTransfer></filter>' +
    '<filter id="wasabi-text-hard"><feComponentTransfer><feFuncA type="discrete" tableValues="0 0 0 1"/></feComponentTransfer></filter>';
  document.body.appendChild(svg);
}

