// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import XuiElement from "./XuiElement";

/**
 * Wasabi:TitleBar. Spotiamp+: an XUI element as Wasabi had it: params it
 * doesn't know (a frame's padtitleleft / padtitleright) go to its scripts'
 * onSetXuiParam, where a skin's titlebar.maki lays its title out with them.
 * Webamp moved and narrowed the bar itself for them, so the title sat that
 * far off (10 px in Winamp Modern's playlist).
 */
export default class WasabiTitleBar extends XuiElement {
  static GUID = "7DFD32444e7c3751AE8240BF33DC3A5F";
}
