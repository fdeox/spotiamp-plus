<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { emitWindowEvent } from "$lib/events.svelte.js";
  import { makeTauriWindowDraggable } from "$lib/window-docking.svelte.js";

  let canvas;

  const close = () => invoke("set_visualizer_window_visible", { visible: false });

  const FRAG = `
    precision highp float;
    uniform vec2 iResolution;
    uniform float iTime;
    uniform float uBass;
    uniform float uMid;
    uniform float uTreble;
    uniform float uLevel;

    // audio-reactive swirling tunnel / plasma — milkdrop-ish
    void main() {
      vec2 uv = (gl_FragCoord.xy - 0.5 * iResolution) / iResolution.y;
      float t = iTime * 0.25;

      float r = length(uv);
      float a = atan(uv.y, uv.x);

      // warp the radius with the beat
      r += sin(a * (5.0 + floor(uMid * 6.0)) + t * 2.0) * (0.06 + 0.22 * uMid);
      r -= uBass * 0.25;

      float tunnel = 0.35 / (r + 0.18) + t * (1.0 + uBass);
      float spokes = a * (2.0 + uTreble * 6.0);

      vec3 col;
      col.r = 0.5 + 0.5 * sin(tunnel * 3.0 + spokes + uBass * 5.0);
      col.g = 0.5 + 0.5 * sin(tunnel * 2.0 + t * 1.3 + uMid * 4.0 + 2.1);
      col.b = 0.5 + 0.5 * sin(tunnel * 4.0 + uTreble * 6.0 + 4.2);

      // ripple detail
      col += 0.15 * uTreble * sin(tunnel * 12.0 + spokes * 2.0);

      // pulse and center bloom
      col *= 0.35 + 0.9 * uLevel + 0.5 * uBass;
      col += (0.25 + uBass) * smoothstep(0.5, 0.0, r);

      // soft vignette
      col *= smoothstep(1.5, 0.15, r);

      gl_FragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
    }
  `;

  const VERT = `
    attribute vec2 p;
    void main() { gl_Position = vec4(p, 0.0, 1.0); }
  `;

  function compile(gl, type, src) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) {
      console.error("shader error", gl.getShaderInfoLog(s));
    }
    return s;
  }

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(320, 240);
    REACTIVE_WINDOW_SIZE.setZoom(1);

    const gl = canvas.getContext("webgl", { antialias: false });
    if (!gl) {
      console.error("WebGL unavailable");
      return;
    }

    const prog = gl.createProgram();
    gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, VERT));
    gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, FRAG));
    gl.linkProgram(prog);
    gl.useProgram(prog);

    // fullscreen quad
    const buf = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buf);
    gl.bufferData(
      gl.ARRAY_BUFFER,
      new Float32Array([-1, -1, 1, -1, -1, 1, -1, 1, 1, -1, 1, 1]),
      gl.STATIC_DRAW,
    );
    const pLoc = gl.getAttribLocation(prog, "p");
    gl.enableVertexAttribArray(pLoc);
    gl.vertexAttribPointer(pLoc, 2, gl.FLOAT, false, 0, 0);

    const uRes = gl.getUniformLocation(prog, "iResolution");
    const uTime = gl.getUniformLocation(prog, "iTime");
    const uBass = gl.getUniformLocation(prog, "uBass");
    const uMid = gl.getUniformLocation(prog, "uMid");
    const uTreble = gl.getUniformLocation(prog, "uTreble");
    const uLevel = gl.getUniformLocation(prog, "uLevel");

    let bass = 0,
      mid = 0,
      treble = 0,
      level = 0;
    let running = true;
    const start = performance.now();

    // poll the Rust spectrum on its own cadence, decoupled from rendering
    function poll() {
      if (!running) return;
      invoke("take_latest_spectrum", {})
        .then((data) => {
          if (Array.isArray(data) && data.length) {
            const v = data.map((pr) => Math.min(Math.max(pr[1] ?? 0, 0), 1));
            const n = v.length;
            const band = (lo, hi) => {
              let s = 0,
                c = 0;
              for (let i = lo; i < hi && i < n; i++) {
                s += v[i];
                c++;
              }
              return c ? s / c : 0;
            };
            const tb = band(0, Math.max(1, Math.floor(n * 0.16)));
            const tm = band(Math.floor(n * 0.16), Math.floor(n * 0.55));
            const tt = band(Math.floor(n * 0.55), n);
            const tl = band(0, n);
            bass += (tb - bass) * 0.35;
            mid += (tm - mid) * 0.35;
            treble += (tt - treble) * 0.35;
            level += (tl - level) * 0.25;
          } else {
            bass *= 0.94;
            mid *= 0.94;
            treble *= 0.94;
            level *= 0.94;
          }
        })
        .catch(() => {})
        .finally(() => {
          if (running) pollTimer = setTimeout(poll, 33);
        });
    }
    let pollTimer = setTimeout(poll, 33);

    function resize() {
      const dpr = Math.min(window.devicePixelRatio || 1, 2);
      const w = Math.floor(canvas.clientWidth * dpr);
      const h = Math.floor(canvas.clientHeight * dpr);
      if (canvas.width !== w || canvas.height !== h) {
        canvas.width = w;
        canvas.height = h;
      }
      gl.viewport(0, 0, canvas.width, canvas.height);
    }

    let raf;
    function frame() {
      if (!running) return;
      resize();
      gl.uniform2f(uRes, canvas.width, canvas.height);
      gl.uniform1f(uTime, (performance.now() - start) / 1000);
      gl.uniform1f(uBass, bass);
      gl.uniform1f(uMid, mid);
      gl.uniform1f(uTreble, treble);
      gl.uniform1f(uLevel, level);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      raf = requestAnimationFrame(frame);
    }
    raf = requestAnimationFrame(frame);

    return () => {
      running = false;
      cancelAnimationFrame(raf);
      clearTimeout(pollTimer);
    };
  });

  function makeVizDraggable(element) {
    makeTauriWindowDraggable(element, {
      async onStart() {
        await emitWindowEvent("visualizerWindow", { DragStarted: null });
        return {};
      },
      async onEnd() {
        await emitWindowEvent("visualizerWindow", { DragEnded: null });
      },
    });
  }

  function makeVizResizable(element) {
    element.onpointerdown = function (event) {
      event.preventDefault();
      element.setPointerCapture(event.pointerId);
      document.onpointermove = function (e) {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        const width = Math.max(Math.round(e.clientX / zoom) + 3, 180);
        const height = Math.max(Math.round(e.clientY / zoom) + 3, 140);
        REACTIVE_WINDOW_SIZE.setSize(width, height);
      };
      document.onpointerup = function () {
        document.onpointermove = null;
        element.releasePointerCapture(event.pointerId);
      };
    };
    element.onselectstart = () => false;
  }
</script>

<div class="viz-window">
  <div class="viz-titlebar" use:makeVizDraggable>
    <div class="viz-tl"></div>
    <span class="viz-title">VISUALIZER</span>
    <button
      class="viz-close"
      onpointerdown={(e) => e.stopPropagation()}
      onclick={close}
      aria-label="Close"
    ></button>
  </div>

  <canvas bind:this={canvas} class="viz-canvas"></canvas>

  <div class="viz-resize" use:makeVizResizable></div>
</div>

<style>
  :global(body) {
    margin: 0;
    overflow: hidden;
    background: #000;
  }

  .viz-window {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: #000;
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #34384a, inset -1px -1px 0 #0e0f16;
    user-select: none;
  }

  /* authentic gen.bmp titlebar (same tiles as the library window) */
  .viz-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: url(/src/static/assets/skins/base-2.91/gen-tiles/gen_fill.png)
      repeat-x;
    cursor: default;
  }
  .viz-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: url(/src/static/assets/skins/base-2.91/gen-tiles/gen_tl.png)
      no-repeat;
  }
  .viz-title {
    position: absolute;
    left: 50%;
    top: 3px;
    transform: translateX(-50%);
    height: 14px;
    display: flex;
    align-items: center;
    padding: 0 8px;
    font-family: "Segoe UI", Tahoma, sans-serif;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 2px;
    color: #cdd6ea;
    background: #26264a;
    z-index: 1;
  }
  .viz-close {
    position: absolute;
    right: 0;
    top: 0;
    width: 15px;
    height: 20px;
    background: url(/src/static/assets/skins/base-2.91/gen-tiles/gen_tr.png)
      no-repeat;
    border: none;
    padding: 0;
    cursor: pointer;
    z-index: 2;
  }

  .viz-canvas {
    flex: 1;
    width: 100%;
    min-height: 0;
    display: block;
    background: #000;
  }

  .viz-resize {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    z-index: 20;
  }
</style>
