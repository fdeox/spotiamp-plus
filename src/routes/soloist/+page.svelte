<script>
  // Spotify Soloist, set up in three steps (soloist.rs): get it from Spotify,
  // the user's own key, a pairing from a Spotify app; then Spotify songs can
  // play through it, for accounts Spotify refuses Spotiamp+'s own player.
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import GenFrame from "../../GenFrame.svelte";

  REACTIVE_WINDOW_SIZE.setSize(330, 440);

  /** @type {{ installed: boolean, version: string, days_left: number | null, has_key: boolean, paired: boolean, running: boolean, on: boolean, device_name: string } | null} */
  let s = $state(null);
  let busy = $state("");
  let error = $state("");
  let progress = $state("");
  let key = $state("");

  const refresh = async () => {
    s = /** @type {any} */ (await invoke("soloist_status").catch(() => null));
  };
  /** @param {string} what @param {() => Promise<any>} job */
  async function run(what, job) {
    busy = what;
    error = "";
    try {
      await job();
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
      progress = "";
      await refresh();
    }
  }
  const download = () => run("download", () => invoke("soloist_install"));
  const openKeyPage = () => invoke("open_external", { target: "soloist-key" }).catch(() => {});
  const saveKey = () =>
    run("key", async () => {
      await invoke("soloist_set_key", { key });
      key = "";
    });
  const pair = () =>
    run("pair", async () => {
      const paired = await invoke("soloist_pair");
      if (!paired) error = "Not paired: it wasn't picked in Spotify in time. Try again?";
    });
  const cancelPair = () => invoke("soloist_cancel_pair").catch(() => {});
  /** @param {boolean} on */
  const setMode = (on) => run("mode", () => invoke("soloist_set_mode", { on }));
  const close = () => invoke("close_soloist_setup").catch(() => {});

  /** @param {number} n */
  const mb = (n) => (n / 1048576).toFixed(1);

  onMount(() => {
    refresh();
    const offs = [
      listen("soloistDownload", (e) => {
        const p = /** @type {{ done: number, total: number }} */ (e.payload);
        progress = p.total ? `${mb(p.done)} of ${mb(p.total)} MB` : `${mb(p.done)} MB`;
      }),
      listen("soloistReady", refresh),
      listen("soloistStopped", refresh),
    ];
    return () => offs.forEach((o) => o.then((f) => f()).catch(() => {}));
  });

  /** @param {MouseEvent} e */
  function drag(e) {
    if (e.button !== 0) return;
    if (/** @type {HTMLElement} */ (e.target).closest("[data-no-drag]")) return;
    getCurrentWindow().startDragging().catch(() => {});
  }
  /** @param {KeyboardEvent} e */
  function onKey(e) {
    if (e.key === "Escape" && !busy) close();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="so" data-gen-window>
  <GenFrame title="Spotify Soloist" onclose={close} drag={(el) => el.addEventListener("mousedown", drag)} />

  <div class="so-body">
    <p>
      Spotify refuses some accounts the songs in every app like Spotiamp+. <b>Soloist</b> is Spotify's own player: set it up once and Spotify songs play
      through it, quietly in the background, with Spotiamp+ as their Winamp. It needs Spotify Premium.
    </p>

    <h2 class:done={s?.installed}>1. Get Soloist from Spotify</h2>
    {#if s?.installed}
      <p class="ok">
        ✓ {s.version || "Installed"}{#if s.days_left != null}. A new one comes by itself before it runs out in {Math.max(0, s.days_left)} days.{/if}
      </p>
    {:else}
      <p>Spotify doesn't let apps include it, so Spotiamp+ downloads it from Spotify for you (about 10 MB).</p>
    {/if}
    <div class="row">
      <button class="so-btn" onclick={download} disabled={!!busy}>{busy === "download" ? progress || "Downloading…" : s?.installed ? "Download again" : "Download"}</button>
    </div>

    <h2 class:done={s?.has_key}>2. Your Soloist key</h2>
    {#if s?.has_key}
      <p class="ok">✓ Saved on this computer. A new one replaces it.</p>
    {:else}
      <p>Make your own on Spotify's developer site: sign in with your Spotify account, accept the terms and generate a key. Then paste it here.</p>
    {/if}
    <div class="row">
      <button class="so-btn" onclick={openKeyPage}>Open the page</button>
      <input class="so-input" type="password" placeholder="paste the key" bind:value={key} spellcheck="false" autocomplete="off" data-no-drag />
      <button class="so-btn" onclick={saveKey} disabled={!!busy || !key.trim()}>Save</button>
    </div>

    <h2 class:done={s?.paired}>3. Pair it once</h2>
    {#if busy === "pair"}
      <p class="wait">
        Now open Spotify on your phone or on this computer (the same network), open the device list and pick <b>{s?.device_name}</b>. Spotiamp+ waits for it.
      </p>
      <div class="row"><button class="so-btn" onclick={cancelPair}>Cancel</button></div>
    {:else}
      {#if s?.paired}
        <p class="ok">✓ Paired. It remembers your account.</p>
      {:else}
        <p>It shows up in Spotify's device list once, as <b>{s?.device_name ?? "Spotiamp+"}</b>, for you to pick.</p>
      {/if}
      <div class="row">
        <button class="so-btn" onclick={pair} disabled={!!busy || !s?.installed || !s?.has_key}>{s?.paired ? "Pair again" : "Pair"}</button>
      </div>
    {/if}

    <h2 class:done={s?.on}>4. Play through Soloist</h2>
    <p>
      {#if s?.on}
        <span class="ok">✓ On{s.running ? "" : ", starting…"}.</span>
      {/if}
      Spotify songs play through it. The equalizer can't reach its sound; the visualizer listens to your speakers.
    </p>
    <div class="row">
      {#if s?.on}
        <button class="so-btn" onclick={() => setMode(false)} disabled={!!busy}>Turn off</button>
      {:else}
        <button class="so-btn" onclick={() => setMode(true)} disabled={!!busy || !s?.installed || !s?.has_key || !s?.paired}>Turn on</button>
      {/if}
    </div>

    {#if error}
      <p class="err">{error}</p>
    {/if}
  </div>

  <div class="so-footer">
    <button class="so-btn so-ok" onclick={close} disabled={busy === "pair"}>Close</button>
  </div>
</div>

<style>
  @font-face {
    font-family: px sans nouveaux;
    font-style: normal;
    font-weight: 400;
    src:
      local("px sans nouveaux"),
      url(/src/static/assets/px_sans_nouveaux.woff) format("woff");
  }
  :global(html),
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }
  .so {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    background: #000;
    box-sizing: border-box;
    padding: 20px 8px 14px 11px;
    isolation: isolate;
    user-select: none;
  }
  .so-body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 9px 8px;
    background: var(--skin-plbg, #000);
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16, inset -1px -1px 0 #3a3f52;
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(8px * var(--px-crisp, 1));
    -webkit-font-smoothing: none;
    line-height: 1.5;
    color: var(--skin-plnormal, #00ff41);
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--skin-plnormal, #00ff41) 40%, transparent) transparent;
  }
  p {
    margin: 0 0 4px;
  }
  b {
    color: var(--skin-plcurrent, #fff);
  }
  h2 {
    margin: 9px 0 2px;
    font-size: calc(8px * var(--px-crisp, 1));
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 1px;
    color: var(--skin-plcurrent, #fff);
  }
  h2.done {
    opacity: 0.75;
  }
  .ok {
    color: var(--skin-plcurrent, #fff);
  }
  .wait {
    color: var(--skin-plcurrent, #fff);
  }
  .err {
    margin-top: 8px;
    color: #ff6a5a;
  }
  .row {
    display: flex;
    gap: 5px;
    align-items: center;
    margin: 2px 0 2px;
  }
  .so-input {
    flex: 1;
    min-width: 0;
    font-family: inherit;
    font-size: calc(8px * var(--px-crisp, 1));
    padding: 2px 4px;
    color: var(--skin-plnormal, #00ff41);
    background: #000;
    border: 1px solid color-mix(in srgb, var(--skin-plnormal, #00ff41) 45%, transparent);
    outline: none;
    user-select: text;
  }
  .so-footer {
    display: flex;
    justify-content: flex-end;
    padding: 5px 4px 3px;
  }
  .so-btn {
    font-family: "px sans nouveaux", sans-serif;
    font-size: calc(8px * var(--px-crisp, 1));
    -webkit-font-smoothing: none;
    padding: 2px 10px;
    white-space: nowrap;
    color: var(--skin-titletext, var(--skin-genexbtntext, #fff));
    background: color-mix(in srgb, var(--frame) 88%, #fff);
    border: 1px solid;
    border-color: color-mix(in srgb, var(--frame) 60%, #fff) color-mix(in srgb, var(--frame) 50%, #000) color-mix(in srgb, var(--frame) 50%, #000)
      color-mix(in srgb, var(--frame) 60%, #fff);
    cursor: pointer;
  }
  .so-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .so-btn:active:not(:disabled) {
    border-color: color-mix(in srgb, var(--frame) 50%, #000) color-mix(in srgb, var(--frame) 60%, #fff) color-mix(in srgb, var(--frame) 60%, #fff)
      color-mix(in srgb, var(--frame) 50%, #000);
  }
  .so-ok {
    min-width: 54px;
  }
</style>
