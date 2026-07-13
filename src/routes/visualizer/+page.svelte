<script>
  import { invoke } from "@tauri-apps/api/core";
  import { Window } from "@tauri-apps/api/window";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import {
    emitWindowEvent,
    subscribeToWindowEvent,
  } from "$lib/events.svelte.js";
  import {
    makeTauriWindowDraggable,
    isDocked,
    rectFromPositionAndSize,
    SNAP_DISTANCE,
    snapPosition,
    STICKY_SNAP_DISTANCE,
  } from "$lib/window-docking.svelte.js";

  let canvas;
  const MODE_NAMES = [
    "tunnel",
    "kaleido",
    "warpgrid",
    "starburst",
    "spiral",
    "plasma",
    "rings",
    "hexgrid",
    "cells",
    "lightning",
    "checker",
    "bars",
    "moire",
    "flow",
    "polygon",
    "waveform",
    "marble",
    "neongrid",
    "metaball",
    "mandala",
  ];
  const MODE_COUNT = MODE_NAMES.length;
  let mode = $state(0);
  const nextMode = () => (mode = (mode + 1) % MODE_COUNT);

  const close = () => invoke("set_visualizer_window_visible", { visible: false });

  const FRAG = `
    precision highp float;
    uniform vec2 iResolution;
    uniform float iTime;
    uniform float uBass;
    uniform float uMid;
    uniform float uTreble;
    uniform float uLevel;
    uniform float uMode;

    vec3 hsv(float h, float s, float v) {
      vec3 rgb = clamp(abs(mod(h*6.0+vec3(0.0,4.0,2.0),6.0)-3.0)-1.0,0.0,1.0);
      return v * mix(vec3(1.0), rgb, s);
    }
    float hash(vec2 p){ return fract(sin(dot(p,vec2(41.3,289.1)))*43758.5); }
    float noise(vec2 p){
      vec2 i=floor(p), f=fract(p); f=f*f*(3.0-2.0*f);
      return mix(mix(hash(i),hash(i+vec2(1.0,0.0)),f.x),
                 mix(hash(i+vec2(0.0,1.0)),hash(i+vec2(1.0,1.0)),f.x), f.y);
    }
    float fbm(vec2 p){ float v=0.0, a=0.5; for(int k=0;k<4;k++){ v+=a*noise(p); p*=2.0; a*=0.5; } return v; }

    void main() {
      vec2 uv = (gl_FragCoord.xy - 0.5*iResolution)/iResolution.y;
      float t = iTime*0.25;
      float r = length(uv);
      float a = atan(uv.y, uv.x);
      int m = int(uMode + 0.5);
      vec3 col = vec3(0.0);

      if (m == 0) {
        float rr = r + sin(a*(5.0+floor(uMid*6.0))+t*2.0)*(0.06+0.22*uMid) - uBass*0.25;
        float tun = 0.35/(rr+0.18)+t*(1.0+uBass);
        col.r=0.5+0.5*sin(tun*3.0+a*2.0+uBass*5.0);
        col.g=0.5+0.5*sin(tun*2.0+t*1.3+uMid*4.0+2.1);
        col.b=0.5+0.5*sin(tun*4.0+uTreble*6.0+4.2);
        col+=(0.25+uBass)*smoothstep(0.5,0.0,rr);
      } else if (m == 1) {
        float aa = abs(mod(a, 1.0472) - 0.5236);
        vec2 p = vec2(cos(aa), sin(aa))*r;
        float v = sin(p.x*10.0+t*3.0+uBass*6.0)*cos(p.y*10.0-t*2.0);
        col = hsv(fract(v*0.3+t*0.2+uMid), 0.8, 0.5+0.5*abs(v)+uLevel*0.5);
      } else if (m == 2) {
        vec2 p = uv*(2.0+uBass*2.0);
        p += 0.3*vec2(sin(p.y*3.0+t*2.0), cos(p.x*3.0+t*1.7))*(0.5+uMid);
        float g = abs(sin(p.x*6.0))*abs(sin(p.y*6.0));
        col = hsv(fract(t*0.1+length(p)*0.1+uTreble), 0.7, g+uLevel*0.4);
      } else if (m == 3) {
        float w = sin(r*20.0 - t*5.0 - uBass*10.0)*0.5+0.5;
        float rays = 0.5+0.5*sin(a*(8.0+floor(uTreble*10.0))+t);
        col = hsv(fract(a/6.2831 + t*0.1), 0.6, w*rays*(0.4+uLevel+uBass));
      } else if (m == 4) {
        // logarithmic spiral galaxy
        float s = sin(a*3.0 + log(r+0.08)*8.0 - t*3.0 - uBass*8.0);
        col = hsv(fract(0.6+t*0.1+uMid), 0.8, 0.5+0.5*s) * (0.5+uLevel+uBass*0.5);
      } else if (m == 5) {
        // classic sine plasma
        float v = sin(uv.x*8.0+t*2.0)+sin(uv.y*8.0+t*1.5)+sin((uv.x+uv.y)*8.0+t)+sin(r*10.0-t*2.0-uBass*8.0);
        col = hsv(fract(v*0.1+t*0.05+uTreble), 0.7, 0.5+0.4*sin(v+uBass*4.0)+uLevel*0.3);
      } else if (m == 6) {
        // pulsing concentric rings
        float ring = sin(r*(14.0+uBass*20.0) - t*4.0);
        col = hsv(fract(r*0.5-t*0.1+uMid), 0.75, smoothstep(0.0,1.0,ring)*(0.4+uLevel+uBass));
      } else if (m == 7) {
        // hexagon grid
        vec2 p = uv*(4.0+uBass*3.0);
        vec2 h = abs(fract(p)-0.5);
        float d = abs(max(h.x*0.866+h.y*0.5, h.y)-0.4);
        col = hsv(fract(t*0.1+uTreble+dot(floor(p),vec2(0.1))), 0.7, smoothstep(0.1,0.0,d)*(0.5+uLevel+uBass*0.5));
      } else if (m == 8) {
        // voronoi cells
        vec2 p = uv*(3.0+uBass*2.0)+t;
        vec2 g = floor(p); float md = 1.0;
        for (int j=-1;j<=1;j++) for (int i=-1;i<=1;i++) {
          vec2 o = vec2(float(i),float(j));
          vec2 pt = o + vec2(hash(g+o),hash(g+o+7.0)) - fract(p);
          md = min(md, length(pt));
        }
        col = hsv(fract(md+t*0.1+uMid), 0.7, (1.0-md)*(0.4+uLevel+uBass));
      } else if (m == 9) {
        // radial lightning
        float b = 0.02/abs(sin(a*3.0+t)*0.5 - r + 0.3 + uBass*0.3);
        col = hsv(fract(0.55+uTreble), 0.5, b*(0.5+uLevel)) + vec3(0.1,0.2,0.4)*b;
      } else if (m == 10) {
        // warped checkerboard
        vec2 p = uv*(3.0+uMid*3.0);
        p *= mat2(cos(t),-sin(t),sin(t),cos(t));
        p += 0.2*sin(p.yx*4.0+t*2.0+uBass*6.0);
        float c = mod(floor(p.x)+floor(p.y), 2.0);
        col = hsv(fract(t*0.1+uTreble), 0.6, (0.2+0.8*c)*(0.4+uLevel+uBass*0.5));
      } else if (m == 11) {
        // radial spectrum bars
        float bars = step(0.5, fract(a*(6.0+floor(uMid*8.0))/6.2831));
        float lvl = 0.3+uBass*0.6+uTreble*0.4;
        col = hsv(fract(a/6.2831+t*0.2), 0.8, step(r,lvl)*bars*(0.6+uLevel));
      } else if (m == 12) {
        // moire interference
        vec2 p = uv*20.0;
        float g1 = sin(p.x*cos(t)+p.y*sin(t));
        float g2 = sin(p.x*cos(t+uBass)+p.y*sin(t+uBass)+t*3.0);
        float mo = g1*g2;
        col = hsv(fract(mo*0.3+t*0.1+uMid), 0.7, 0.5+0.5*mo+uLevel*0.4);
      } else if (m == 13) {
        // flowing fbm noise
        vec2 p = uv*2.0;
        p += vec2(fbm(p+t), fbm(p-t))*(1.0+uBass);
        float f = fbm(p*2.0+t);
        col = hsv(fract(f+t*0.1+uTreble), 0.7, f*(0.5+uLevel+uBass));
      } else if (m == 14) {
        // rotating polygon rings
        float n = 3.0+floor(uMid*6.0);
        float ang = 6.2831/n;
        float d = cos(floor(0.5+a/ang)*ang - a)*r;
        float poly = smoothstep(0.03,0.0, abs(fract(d*(6.0+uBass*8.0)-t)-0.5));
        col = hsv(fract(a/6.2831+t*0.1), 0.7, poly*(0.5+uLevel+uBass));
      } else if (m == 15) {
        // radial audio waveform ring
        float wave = 0.35 + 0.12*sin(a*8.0+t*4.0) + 0.18*uBass + 0.1*sin(a*20.0-t*6.0)*uTreble;
        col = hsv(fract(a/6.2831+t*0.2), 0.8, smoothstep(0.05,0.0, abs(r-wave))*(0.6+uLevel));
      } else if (m == 16) {
        // marble
        float mrb = sin((uv.x + fbm(uv*3.0+t))*8.0 + uBass*6.0);
        col = hsv(fract(mrb*0.2+t*0.1+uMid), 0.6, 0.5+0.5*mrb+uLevel*0.3);
      } else if (m == 17) {
        // neon perspective grid
        vec2 p = uv; p.y += 0.55;
        float persp = 1.0/(abs(p.y)+0.08);
        vec2 g = vec2(p.x*persp, persp*(1.0+uBass) + t*3.0);
        float line = max(smoothstep(0.06,0.0,abs(fract(g.x)-0.5)), smoothstep(0.06,0.0,abs(fract(g.y)-0.5)));
        col = hsv(fract(0.6+t*0.1+uTreble), 0.85, line*(0.4+uLevel+uBass)) * step(0.0,p.y);
      } else if (m == 18) {
        // metaballs
        float mb = 0.0;
        for (int k=0;k<4;k++) {
          float fk = float(k);
          vec2 c = 0.5*vec2(sin(t*(1.0+fk*0.3)+fk), cos(t*(0.8+fk*0.2)+fk*2.0));
          mb += (0.09+uBass*0.09)/length(uv-c);
        }
        col = hsv(fract(mb*0.1+t*0.1+uMid), 0.7, smoothstep(1.0,2.2,mb)*(0.6+uLevel));
      } else {
        // mandala
        float seg = 8.0+floor(uMid*8.0);
        float aa = abs(mod(a, 6.2831/seg) - 3.1415/seg);
        float mand = sin(aa*10.0+t)*sin(r*15.0 - t*3.0 - uBass*8.0);
        col = hsv(fract(r+t*0.1+uTreble), 0.8, 0.5+0.5*mand+uLevel*0.4);
      }

      col *= 0.35+0.9*uLevel+0.3*uBass;
      col *= smoothstep(1.5, 0.15, r);
      gl_FragColor = vec4(clamp(col,0.0,1.0),1.0);
    }
  `;

  const VERT = `attribute vec2 p; void main(){ gl_Position = vec4(p,0.0,1.0); }`;

  function compile(gl, type, src) {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS))
      console.error("shader", gl.getShaderInfoLog(s));
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

    const u = (n) => gl.getUniformLocation(prog, n);
    const uRes = u("iResolution"),
      uTime = u("iTime"),
      uBassL = u("uBass"),
      uMidL = u("uMid"),
      uTrebL = u("uTreble"),
      uLevelL = u("uLevel"),
      uModeL = u("uMode");

    let bass = 0,
      mid = 0,
      treble = 0,
      level = 0,
      running = true;
    const start = performance.now();

    let pollTimer = setTimeout(function poll() {
      if (!running) return;
      invoke("take_latest_spectrum", {})
        .then((data) => {
          if (Array.isArray(data) && data.length) {
            const v = data.map((pr) => Math.min(Math.max(pr[1] ?? 0, 0), 1));
            const n = v.length;
            const band = (lo, hi) => {
              let s = 0,
                c = 0;
              for (let i = lo; i < hi && i < n; i++) (s += v[i]), c++;
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
    }, 33);

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
      gl.uniform1f(uBassL, bass);
      gl.uniform1f(uMidL, mid);
      gl.uniform1f(uTrebL, treble);
      gl.uniform1f(uLevelL, level);
      gl.uniform1f(uModeL, mode);
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      raf = requestAnimationFrame(frame);
    }
    raf = requestAnimationFrame(frame);

    // auto-cycle presets, and switch on every track change for milkdrop variety
    const cycle = setInterval(nextMode, 25000);
    let lastUri = "";
    let unsub;
    subscribeToWindowEvent("player", (event) => {
      const p = event.Playing;
      if (p && p.uri && p.uri !== lastUri) {
        lastUri = p.uri;
        mode = Math.floor(Math.random() * MODE_COUNT);
      }
    }).then((u2) => (unsub = u2));

    return () => {
      running = false;
      cancelAnimationFrame(raf);
      clearTimeout(pollTimer);
      clearInterval(cycle);
      if (unsub) unsub();
    };
  });

  function makeVizDraggable(element) {
    makeTauriWindowDraggable(element, {
      async onStart({ startPosition, windowSize }) {
        const playerWindow = await Window.getByLabel("player");
        if (!playerWindow) return false;
        await emitWindowEvent("visualizerWindow", { DragStarted: null });
        const [pp, ps] = await Promise.all([
          playerWindow.outerPosition(),
          playerWindow.outerSize(),
        ]);
        const playerRect = rectFromPositionAndSize(pp, ps);
        return {
          playerRect,
          vizSize: windowSize,
          docked: isDocked(
            rectFromPositionAndSize(startPosition, windowSize),
            playerRect,
          ),
        };
      },
      mapPosition(rawPosition, context) {
        const rawRect = {
          ...rawPosition,
          width: context.vizSize.width,
          height: context.vizSize.height,
        };
        const d = context.docked ? STICKY_SNAP_DISTANCE : SNAP_DISTANCE;
        const snapped = snapPosition(rawRect, context.playerRect, d);
        context.docked = snapped !== undefined;
        return snapped ?? rawPosition;
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
    <button class="viz-close" data-no-drag onclick={close} aria-label="Close"
    ></button>
  </div>

  <div class="viz-stage">
    <canvas
      bind:this={canvas}
      class="viz-canvas"
      onclick={nextMode}
      title="click to change pattern"
    ></canvas>
    <span class="viz-preset">{mode + 1}/{MODE_COUNT} · {MODE_NAMES[mode]}</span>
  </div>

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

  .viz-stage {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .viz-canvas {
    width: 100%;
    height: 100%;
    display: block;
    background: #000;
    cursor: pointer;
  }
  .viz-preset {
    position: absolute;
    left: 4px;
    bottom: 3px;
    font-family: monospace;
    font-size: 9px;
    color: #6effa0;
    text-shadow: 0 0 3px #000, 0 0 2px #000;
    pointer-events: none;
    opacity: 0.75;
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
