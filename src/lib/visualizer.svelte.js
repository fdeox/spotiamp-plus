import { invoke } from "@tauri-apps/api/core";

class Bar {
    value = $state(0);
    hat = $state(0);
    hatVelocity = 0;
    gravity = 0.000005;
    levitateTimeMs = 450;
    /**
     * @param {number} index
     */
    constructor(index) {
        this.index = index;
    }

    reset() {
        this.value = 0;
        this.hat = 0;
        this.hatVelocity = 0;
    }

    /**
     * @param {number} newValue
     */
    setValue(newValue) {
        this.value = newValue;
        if (this.hat <= newValue) {
            this.hat = newValue;
            this.hatVelocity = this.gravity * this.levitateTimeMs;
        }
    }

    /**
     * @param {number} deltaTime
     */
    update(deltaTime) {
        this.hatVelocity = Math.max(
            this.hatVelocity - this.gravity * deltaTime,
            -1,
        );
        if (this.hatVelocity < 0) {
            this.hat = Math.max(0, this.hat + this.hatVelocity * deltaTime);
        }
    }
}
/** Spectrum reads per second, at most. Every read is a round trip to Rust,
 *  and following the display instead meant 144 a second on a 144 Hz screen. */
const MAX_READS_PER_SECOND = 40;

export class Visualizer {
    bars = $state(Array.from({ length: 19 }, (_, index) => new Bar(index)));
    clearAfterStop = false;
    lastTick = 0;
    /** Which update loop is the live one. A loop from before a stop/start
     *  whose read was still out (the read waits while a track loads) sees a
     *  newer number and ends, instead of carrying on beside the new loop —
     *  before, every track change could leave one more loop running. */
    loopId = 0;

    /**
     * @param {string} [spectrumCommand] the backend command polled for spectrum
     *   data — the loopback source in Free Mode, the player sink otherwise.
     */
    constructor(spectrumCommand = "take_latest_spectrum") {
        this.running = false;
        this.spectrumCommand = spectrumCommand;
    }

    /** @param {number} id */
    runVisualizerUpdate(id) {
        if (id !== this.loopId) {
            return;
        }
        if (this.running) {
            const now = Date.now();
            const wait = 1000 / MAX_READS_PER_SECOND - (now - this.lastTick);
            if (wait > 0) {
                // Sleep until the next read is due. Spinning on
                // requestAnimationFrame woke the window at the display rate
                // (165 a second on a 165 Hz screen) just to check the clock.
                setTimeout(() => this.runVisualizerUpdate(id), wait);
                return;
            }
            const deltaTime = now - this.lastTick;
            this.lastTick = now;
            invoke(this.spectrumCommand, {}).then((visualizerData) => {
                if (id !== this.loopId) {
                    return;
                }
                if (Array.isArray(visualizerData)) {
                    visualizerData.forEach((pair, index) => {
                        const bar = this.bars[index];
                        if (!bar) {
                            return;
                        }
                        bar.setValue(Math.min(pair[1], 1));
                        bar.update(deltaTime);
                    });
                }
                requestAnimationFrame(() => this.runVisualizerUpdate(id));
            }).catch((e) => {
                console.error("Failed to fetch visualizer data", e);
                // Try again...
                requestAnimationFrame(() => this.runVisualizerUpdate(id));
            });
        } else {
            if (this.clearAfterStop) {
                this.clear();
            }
        }
    }

    start() {
        // Already looping: nothing to do (a second loop would double the reads).
        if (this.running) {
            return;
        }
        this.running = true;
        // Start the update "loop" using requestAnimationFrame to move forward
        this.lastTick = Date.now();
        this.runVisualizerUpdate(++this.loopId);
    }

    clear() {
        for (const bar of this.bars) {
            bar.reset();
        }
    }

    /**
     * @param {boolean} clearAfterStop
     */
    stop(clearAfterStop) {
        if (!this.running && clearAfterStop) {
            this.clear();
        } else {
            this.running = false;
            this.clearAfterStop = clearAfterStop;
        }
    }
}
