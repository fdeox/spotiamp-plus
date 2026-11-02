// @ts-nocheck: Webamp Modern's code, written for a looser TypeScript setup than Spotiamp+'s
// Spotiamp+: a modern skin plays nothing itself. This stands where Webamp's
// own player was (an <audio> element and Web Audio): what plays, where it is,
// how loud and the equalizer come in from Spotiamp+'s player (../host.ts),
// and the skin's buttons and sliders go out to it. The visualizers read a
// stand-in analyser filled with Spotiamp+'s own spectrum and waveform.
import { Emitter } from "../utils";

export const AUDIO_PAUSED = "paused";
export const AUDIO_STOPPED = "stopped";
export const AUDIO_PLAYING = "playing";

// moved here because used by several files eg Playlist,AudioMetadata
export type Track = {
  id?: number; //used for identification while refreshing playlist
  filename: string; // full url, or just File.name
  file?: File; // Blob
  metadata?: any;
  title?: string;
  rating?: number; // 0..5
  duration?: number | null;
};

/** What the skin's controls ask of Spotiamp+'s player (set by host.ts). */
export type PlayerCommands = {
  play(): void;
  pause(): void;
  stop(): void;
  /** to `ms` into the song */
  seek(ms: number): void;
  /** 0..1 */
  volume(value: number): void;
  /** -1..1 */
  balance(value: number): void;
  /** "preamp" or "1".."10", 0..1 (0.5 is flat) */
  eq(kind: string, value: number): void;
  eqEnabled(on: boolean): void;
};

const nothing = () => {};

/**
 * Spotiamp+'s sound as Web Audio's AnalyserNode shows it: 512 frequency
 * bins and 1024 waveform samples, written by host.ts from the app's own
 * spectrum and waveform, read by the skin's visualizers.
 */
export class RemoteAnalyser {
  fftSize = 1024;
  frequencyBinCount = 512;
  _frequencies = new Uint8Array(512);
  _waveform = new Uint8Array(1024).fill(128);

  getByteFrequencyData(out: Uint8Array) {
    out.set(this._frequencies.subarray(0, out.length));
  }
  getByteTimeDomainData(out: Uint8Array) {
    out.set(this._waveform.subarray(0, out.length));
  }
  getFloatTimeDomainData(out: Float32Array) {
    for (let i = 0; i < out.length; i++) out[i] = (this._waveform[i] - 128) / 128;
  }
  getFloatFrequencyData(out: Float32Array) {
    // 0..255 as -100..-30 dB, Web Audio's default range
    for (let i = 0; i < out.length; i++) out[i] = -100 + (this._frequencies[i] / 255) * 70;
  }
}

export class AudioPlayer {
  _eventListener: Emitter = new Emitter();
  _eqEmitter: Emitter = new Emitter();
  _analyser = new RemoteAnalyser();
  /** Webamp's in-skin MilkDrop (Avs) used this; there's no Web Audio here. */
  _context: any = null;
  _state: string = AUDIO_STOPPED;
  /** seconds */
  _position: number = 0;
  /** seconds, 0 when not known */
  _length: number = 0;
  _volume: number = 1;
  _balance: number = 0;
  _eqEnabled: boolean = true;
  _eqValues: { [kind: string]: number } = {};
  _trackInfo: {} = {};
  _albumArtUrl: string = null;
  _timeRemaining: boolean = false; // to show minus
  _vuMeter: number = 0;
  _playbackRate: number = 1;
  commands: PlayerCommands = {
    play: nothing,
    pause: nothing,
    stop: nothing,
    seek: nothing,
    volume: nothing,
    balance: nothing,
    eq: nothing,
    eqEnabled: nothing,
  };

  get _isStop(): boolean {
    return this._state === AUDIO_STOPPED;
  }

  // ---- in, from Spotiamp+ (host.ts) ----

  _setState(state: string) {
    if (state === this._state) return;
    // (Spotiamp+: playing on after a pause is Winamp's resume, not a new play)
    const resumed = state === AUDIO_PLAYING && this._state === AUDIO_PAUSED;
    this._state = state;
    this.trigger(resumed ? "resume" : state === AUDIO_PLAYING ? "play" : state === AUDIO_PAUSED ? "pause" : "stop");
    this.trigger("statchanged");
  }
  _setPosition(seconds: number) {
    const changed = Math.floor(seconds) !== Math.floor(this._position);
    this._position = seconds;
    if (changed) this.trigger("timeupdate");
  }
  _setLength(seconds: number) {
    if (seconds === this._length) return;
    this._length = seconds;
    this.trigger("timeupdate");
  }
  _setVolume(volume: number) {
    if (volume === this._volume) return;
    this._volume = volume;
    this.trigger("volumechanged");
  }
  _setBalance(balance: number) {
    if (balance === this._balance) return;
    this._balance = balance;
    this.trigger("balancechange");
  }
  _setEq(kind: string, value: number) {
    if (this._eqValues[kind] === value) return;
    this._eqValues[kind] = value;
    this._eqEmitter.trigger(kind);
  }
  _setEqEnabled(on: boolean) {
    if (on === this._eqEnabled) return;
    this._eqEnabled = on;
    this.trigger("eqenabled");
  }

  // ---- what the skin uses ----

  doTimeUpdate() {
    this.trigger("timeupdate");
  }

  setEqEnabled(enable: boolean) {
    this._setEqEnabled(enable);
    this.commands.eqEnabled(enable);
  }
  getEqEnabled(): boolean {
    return this._eqEnabled;
  }

  getAnalyser(): any {
    return this._analyser;
  }

  on(event: string, callback: Function): Function {
    return this._eventListener.on(event, callback);
  }
  trigger(event: string, ...args: any[]) {
    this._eventListener.trigger(event, ...args);
  }
  off(event: string, callback: Function) {
    this._eventListener.off(event, callback);
  }

  /** Webamp loaded a file here; in Spotiamp+ the playlist does that. */
  setAudioSource(url: string) {}

  // 0-1
  getVolume(): number {
    return this._volume;
  }
  setVolume(volume: number) {
    const v = Math.min(1, Math.max(0, volume));
    this._setVolume(v);
    this.commands.volume(v);
  }

  getBalance(): number {
    return this._balance;
  }
  // -1..0..1
  setBalance(balance: number) {
    const b = Math.min(1, Math.max(-1, balance));
    this._setBalance(b);
    this.commands.balance(b);
  }

  getPlaybackRate(): number {
    return this._playbackRate;
  }
  setPlaybackRate(value: number) {
    // Spotiamp+ plays at normal speed only
    this.trigger("playbackratechange");
  }

  play() {
    this.commands.play();
  }
  stop() {
    this.commands.stop();
  }
  pause() {
    this.commands.pause();
  }

  seekTo(secs: number) {
    this.commands.seek(Math.max(0, secs) * 1000);
  }
  seekToPercent(percent: number) {
    if (this._length > 0) this.seekTo(this._length * percent);
  }

  toggleRemainingTime() {
    this._timeRemaining = !this._timeRemaining;
    this.trigger("timeupdate");
  }
  // In seconds
  getCurrentTime(): number {
    return this._timeRemaining && this._length > 0 ? this._position - this._length : this._position;
  }
  getCurrentTimePercent(): number {
    return this._length > 0 ? Math.min(1, this._position / this._length) : 0;
  }

  getState(): string {
    return this._state;
  }

  /** 0..1, 0.5 is flat */
  getEq(kind: string): number {
    return this._eqValues[kind] ?? 0.5;
  }
  setEq(kind: string, value: number) {
    const v = Math.min(1, Math.max(0, value));
    this._setEq(kind, v);
    this.commands.eq(kind, v);
  }
  onEqChange(kind: string, cb: () => void): Function {
    return this._eqEmitter.on(kind, cb);
  }

  onCurrentTimeChange(cb: () => void): Function {
    return this.on("timeupdate", cb);
  }
  onSeek(cb: () => void): () => void {
    const dispose = this.on("seeked", cb);
    return () => dispose();
  }
  onVolumeChanged(cb: Function): Function {
    return this.on("volumechanged", cb);
  }
  onBalanceChanged(cb: () => void): Function {
    return this.on("balancechange", cb);
  }

  // Current track length in seconds
  getLength(): number {
    return this._length;
  }
}

const AUDIO_PLAYER = new AudioPlayer();

export default AUDIO_PLAYER;
