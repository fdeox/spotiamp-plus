// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
import { VisPaintHandler, registerPainter } from "./Vis";
import Vis from "./Vis";
import GuiObj from "./GuiObj";
import { HOST } from "../../host";

/**
 * The skin's big visualizer area (Winamp's AVS). Spotiamp+: Webamp ran
 * MilkDrop in the page here (butterchurn); Spotiamp+ has its own MilkDrop in
 * its visualizer window, so this draws the spectrum, or the waveform after
 * "next", and the skin's fullscreen button opens Spotiamp+'s visualizer.
 */
export default class Avs extends Vis {
  static GUID = "OFFICIALLY-NO-GUID";

  init() {
    this._mode = "spotiamp-avs";
    super.init();
    this._uiRoot._avss.push(this);
  }

  handleAction(
    action: string,
    param: string | null = null,
    actionTarget: string | null = null,
    source: GuiObj = null
  ): boolean {
    action = action.toLowerCase();
    if (["vis_prev", "vis_next", "vis_f5"].includes(action)) {
      (this._painter as AvsPaintHandler).doAction?.(action);
      return true;
    }
    return false;
  }
}

class AvsPaintHandler extends VisPaintHandler {
  _bins = new Uint8Array(512);
  _wave = new Uint8Array(1024);
  _waveform = false;

  prepare() {}

  paintFrame() {
    const canvas = this._vis._canvas;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const { width, height } = canvas;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, width, height);
    const analyser = this._vis._uiRoot.audio.getAnalyser();
    if (this._waveform) {
      analyser.getByteTimeDomainData(this._wave);
      ctx.strokeStyle = "#7fd3ff";
      ctx.lineWidth = Math.max(1, height / 120);
      ctx.beginPath();
      for (let x = 0; x < width; x++) {
        const v = this._wave[Math.floor((x / width) * this._wave.length)];
        const y = height / 2 + ((v - 128) / 128) * (height / 2.2);
        if (x === 0) ctx.moveTo(x, y);
        else ctx.lineTo(x, y);
      }
      ctx.stroke();
      return;
    }
    analyser.getByteFrequencyData(this._bins);
    const bars = Math.max(8, Math.min(64, Math.floor(width / 6)));
    const w = width / bars;
    const gradient = ctx.createLinearGradient(0, height, 0, 0);
    gradient.addColorStop(0, "#1f8f2b");
    gradient.addColorStop(0.6, "#d6d61d");
    gradient.addColorStop(1, "#e2381d");
    ctx.fillStyle = gradient;
    for (let i = 0; i < bars; i++) {
      // low bins get more bars, as the ear hears them
      const from = Math.floor(Math.pow(i / bars, 2) * this._bins.length);
      const to = Math.max(from + 1, Math.floor(Math.pow((i + 1) / bars, 2) * this._bins.length));
      let peak = 0;
      for (let b = from; b < to; b++) peak = Math.max(peak, this._bins[b]);
      const h = (peak / 255) * height;
      ctx.fillRect(Math.floor(i * w) + 1, height - h, Math.max(1, Math.floor(w) - 1), h);
    }
  }

  doAction(action: string) {
    if (action === "vis_f5") HOST.openVisualizer();
    else this._waveform = !this._waveform;
  }
}
registerPainter("spotiamp-avs", AvsPaintHandler);
