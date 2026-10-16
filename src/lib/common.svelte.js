import { message } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";

/** The UI scale this window was created at (set by the Rust side before the
 *  page's first paint, see app_window::build_frameless_window); 1 if absent. */
function initialUiScale() {
    const s = Number(/** @type {any} */ (globalThis).__SPOTIAMP_UI_SCALE__);
    return s >= 1 && s <= 3 ? s : 1;
}

class ReactiveWindowSize {
    width = $state(275.0);
    height = $state(116.0);
    zoom = $state(initialUiScale());

    /**
     * @param {number} width
     * @param {number} height
     */
    setSize(width, height) {
        this.width = width;
        this.height = height;
    }

    /**
     * @param {number} zoom
     */
    setZoom(zoom) {
        this.zoom = zoom;
        const body = document.querySelector("body");
        // Render the content at its natural 1x (crisp), and scale the window
        // root up with CSS `zoom` (--app-zoom). CSS zoom re-rasterises, so
        // `image-rendering: pixelated` keeps the pixel-art sharp — unlike
        // `transform: scale`, which the compositor smooths (blurry at 2x/3x).
        body?.style.setProperty("--zoom", "1");
        body?.style.setProperty("--app-zoom", `${zoom}`);
        crispPixelFont(zoom);
    }
}
export const REACTIVE_WINDOW_SIZE = new ReactiveWindowSize();

/** Sent on the window when a skin has been put on (or taken off): pages
 *  with something of the skin's own to redo (the window's shape) listen. */
export const SKIN_APPLIED = "spotiamp-skin-applied";

/** The pixel font (px sans nouveaux) is drawn on an 8 px grid: it's crisp
 *  only when each of its pixels lands on whole screen pixels. At 125 % (a
 *  laptop's usual scale) 8 px is 10 screen pixels, its pixels 1.25 each, and
 *  Windows smooths them into a blur. --px-crisp sizes it to the nearest
 *  whole multiple of 8 screen pixels instead (8 at 125 %, 16 at 150-249 %),
 *  and follows the window to a screen with another scale. */
let crispWatch = /** @type {MediaQueryList | null} */ (null);
const crispFollow = () => crispPixelFont(REACTIVE_WINDOW_SIZE.zoom);
/** @param {number} zoom the window's UI scale */
function crispPixelFont(zoom) {
    const body = document.querySelector("body");
    if (!body) return;
    const scale = (window.devicePixelRatio || 1) * (zoom || 1);
    const step = Math.max(1, Math.round(scale));
    body.style.setProperty("--px-crisp", `${step / scale}`);
    // one whole step bigger, for text meant to be read at length (lyrics)
    body.style.setProperty("--px-crisp-up", `${(step + 1) / scale}`);
    crispWatch?.removeEventListener("change", crispFollow);
    crispWatch = window.matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    crispWatch.addEventListener("change", crispFollow);
}

export const enterExitViewportObserver = new IntersectionObserver(
    (entries) => {
        entries.forEach((entry) => {
            const eventName = entry.isIntersecting ? "enterViewport" : "exitViewport";
            entry.target.dispatchEvent(new CustomEvent(eventName));
        });
    },
);

/**
 * @param {HTMLElement} element
 */
export function enterExitViewport(element) {
    enterExitViewportObserver.observe(element);

    return {
        destroy() {
            enterExitViewportObserver.unobserve(element);
        },
    };
}

/**
 * @param {number} start
 * @param {number} end
 */
export function* range(start, end) {
    for (let i = start; i <= end; i++) {
        yield i;
    }
}

/**
 * @param {Error | string} e
 */
export async function handleError(e) {
    const COPY = "Copy diagnostic info";
    const choice = await message(`${e}`, {
        title: "Spotiamp+",
        kind: "error",
        buttons: { ok: COPY, cancel: "Close" },
    });
    if (choice === COPY) await copyDiagnostics(`${e}`);
}

/**
 * Put what a bug report needs on the clipboard (version, Windows, audio setup,
 * recent log errors; names and paths scrubbed) and say where to paste it.
 * @param {string} [error] the error the user just saw, if any
 */
export async function copyDiagnostics(error) {
    try {
        await invoke("copy_diagnostics", { error: error ?? null });
        await message(
            "Diagnostic info copied.\n\nPaste it into /bug on the Spotiamp+ Discord or a GitHub issue. Your Spotify name, Windows user name and file paths are left out.",
            { title: "Spotiamp+", kind: "info" },
        );
    } catch (err) {
        await message(`Couldn't copy the diagnostic info (${err}).`, { title: "Spotiamp+", kind: "error" });
    }
}

/**
 * @param {WindowEventMap[keyof WindowEventMap]} ev
 */
function preventAndStopPropagation(ev) {
    ev.preventDefault();
    ev.stopPropagation();
}

/**
 * A file's path as a file: link (a song dragged out of the Library's Local
 * Media carries one), and back.
 * @param {string} path e.g. C:\Music\a b.mp3 or \\server\share\x.mp3
 */
export function toFileUrl(path) {
    const parts = path.replaceAll("\\", "/").split("/");
    const encoded = parts
        .map((p, i) => (i === 0 && /^[A-Za-z]:$/.test(p) ? p : encodeURIComponent(p)))
        .join("/");
    return encoded.startsWith("//") ? `file:${encoded}` : `file:///${encoded}`;
}

/** @param {string} url a file: link */
export function fromFileUrl(url) {
    const u = new URL(url);
    const path = decodeURIComponent(u.pathname).replaceAll("/", "\\");
    return u.host ? `\\\\${u.host}${path}` : path.replace(/^\\(?=[A-Za-z]:)/, "");
}

/**
 * @param {string} uriList
 */
function parseUriList(uriList) {
    return uriList
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter((line) => line && !line.startsWith("#"));
}

/**
 * @param {(urls: string[]) => void} urlCallback
 * @returns {() => void} unlisten
 */
export function handleDrop(urlCallback) {
    window.addEventListener("dragenter", preventAndStopPropagation);
    window.addEventListener("dragover", preventAndStopPropagation);
    window.addEventListener("drop", preventAndStopPropagation);

    /**
     * @param {DocumentEventMap["drop"]} ev
     */
    function documentDropListener(ev) {
        if (ev.dataTransfer) {
            for (const item of ev.dataTransfer.items) {
                if (item.kind === "string" && item.type.match(/^text\/uri-list/)) {
                    item.getAsString((itemText) => {
                        urlCallback(parseUriList(itemText));
                    });
                }
            }
        }
    }
    document.addEventListener("drop", documentDropListener);
    return () => {
        document.removeEventListener("drop", documentDropListener);
        window.removeEventListener("dragenter", preventAndStopPropagation);
        window.removeEventListener("dragover", preventAndStopPropagation);
        window.removeEventListener("drop", preventAndStopPropagation);
    };
}

/**
 * @typedef {{ width: number, height: number }} WindowInnerSize
 */

/**
 * @typedef {{ width: number, height: number }} WindowOuterPosition
 */


/**
 * @typedef {{ inner_size: WindowInnerSize | null, outer_position: WindowOuterPosition | null }} WindowState
 */

/**
 * @typedef {{ volume: number, double_size_active: boolean, windowshade_active?: boolean, controller_mode?: boolean, show_playlist: boolean, show_eq?: boolean, resume?: { uri: string, index: number, position_ms: number } | null, window_state: WindowState }} PlayerSettings
 */

/**
 * @typedef {{ uris: string[], free_uris?: string[], window_state: WindowState, controller_mode?: boolean }} PlaylistSettings
 */
