<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable, makeSnappingResizer } from "$lib/window-docking.svelte.js";

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
    "fractal",
    "nebula",
    "wormhole",
    "warpspeed",
    "liquid",
    "crystal",
    "aurora",
    "vortex",
    "supernova",
    "circuit",
    // Instrument-style displays. These read the spectrum texture directly
    // rather than the four smoothed bands, so they show actual frequency
    // content instead of reacting to an average.
    "spectrum",
    "waterfall",
    "vumeter",
    "scope",
    "radial",
    "ledladder",
    "terrain",
    "strings",
    "matrix",
    "phyllo",
    "voronoi",
    "rain",
    "fireworks",
    "smoke",
    "dna",
    "milkdrop",
    "ripple",
    "comets",
    "ribbon",
    "glitch",
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
    uniform sampler2D uSpec;
    // The previously rendered frame, for the one mode that feeds back into
    // itself. Every other mode ignores it.
    uniform sampler2D uPrev;

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
    // Cosine gradient palette. Gives coherent, art-directed colour ramps
    // instead of the full-rainbow sweep hsv() produces.
    vec3 pal(float x, vec3 a, vec3 b, vec3 c, vec3 d){ return a + b*cos(6.28318*(c*x+d)); }

    // Spectrum texture: 256 wide (frequency bin, low to high) x 64 tall
    // (history, row 0 newest). One texture serves both the live analyser
    // displays and the scrolling spectrogram.
    float spec(float x){ return texture2D(uSpec, vec2(clamp(x,0.0,1.0), 0.0)).r; }
    float specAt(float x, float age){ return texture2D(uSpec, vec2(clamp(x,0.0,1.0), clamp(age,0.0,1.0))).r; }
    // Perceptual spread: low bins get more width, the way a real analyser lays
    // its bands out.
    float specLog(float x){ return spec(pow(clamp(x,0.0,1.0), 1.8)); }

    // How much of the previous frame a mode inherits — the trail character.
    //
    // The instrument displays get none on purpose: trails on an analyser, a VU
    // needle or an LED ladder smear the exact reading those modes exist to
    // give, and the spectrogram already plots time on its own axis. matrix
    // draws its own falling tails, so a second set just muddies it. milkdrop
    // runs its own tuned feedback further down and must not get this one too.
    float feedbackFor(int m) {
      if (m==30 || m==31 || m==32 || m==35 || m==38 || m==49) return 0.0;
      if (m==45) return 0.0;
      if (m==33) return 0.90;   // scope: reads as CRT phosphor persistence
      if (m<30) return 0.87;    // the original set already fills the frame
      return 0.92;
    }

    void main() {
      vec2 uv = (gl_FragCoord.xy - 0.5*iResolution)/iResolution.y;
      vec2 sc = gl_FragCoord.xy / iResolution;   // 0..1, for panel-style displays
      float t = iTime*0.25;
      float r = length(uv);
      float a = atan(uv.y, uv.x);
      int m = int(uMode + 0.5);
      vec3 col = vec3(0.0);

      if (m == 0) {
        // tunnel: panelled walls receding into fog, bass drives the camera
        float zd = 0.45/(r+0.09);
        float depth = zd + t*(1.2+uBass*2.5);
        float ang = a + sin(zd*0.25 + t*0.4)*0.35;
        float rows = smoothstep(0.42,0.0, abs(fract(depth*0.6)-0.5));
        float cols = smoothstep(0.45,0.05, abs(fract(ang*(6.0+floor(uMid*6.0))/6.2831)-0.5));
        float grout = smoothstep(0.06,0.0, abs(fract(depth*0.6)-0.5));
        float fog = exp(-zd*0.11);
        col = pal(fract(depth*0.045 + uMid*0.25),
                  vec3(0.5), vec3(0.45), vec3(1.0,0.95,0.85), vec3(0.0,0.2,0.45));
        col *= (0.2 + rows*cols*1.2 + grout*0.8)*fog;
        col += vec3(1.0,0.75,0.45)*smoothstep(0.32,0.0,r)*(0.25+uBass*1.3);
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
        // ripples: expanding wavefronts, each with a specular crest
        float w = 0.0;
        for (int k=0;k<3;k++) {
          float fk = float(k);
          float rr = fract(t*(0.35+fk*0.12) + fk*0.37);
          w += (1.0-rr)*(0.6+uBass*0.9)
             * exp(-abs(r-rr*1.5)*(14.0-uMid*6.0))
             * sin((r-rr*1.5)*40.0);
        }
        float crest = smoothstep(0.0,0.6,abs(w));
        col = pal(fract(0.55 + r*0.35 - t*0.05 + uMid*0.2),
                  vec3(0.4,0.45,0.55), vec3(0.35,0.4,0.45), vec3(1.0), vec3(0.0,0.2,0.4));
        col *= 0.2 + crest*(1.4+uLevel);
        col += vec3(0.7,0.85,1.0)*pow(crest,6.0)*(0.4+uTreble*1.6);
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
        // lightning: fbm-jittered bolts, hot core inside a cool corona
        float glow = 0.0;
        for (int k=0;k<3;k++) {
          float fk = float(k);
          float seed = fk*7.3 + floor(t*(2.0+uBass*4.0));   // re-strikes on beats
          float jag = (fbm(vec2(uv.y*3.5 + seed, seed*0.7 + t*0.6)) - 0.5)*1.3
                    + (fbm(vec2(uv.y*11.0 + seed, t*1.4)) - 0.5)*0.45;
          float d = abs(uv.x - ((fk-1.0)*0.42 + jag*(0.5+uMid)));
          float flicker = 0.45 + 0.55*hash(vec2(seed, floor(t*14.0)));
          glow += (0.006+uBass*0.012)/(d+0.006)*flicker;
        }
        col = vec3(0.35,0.55,1.0)*glow*0.5;
        col += vec3(1.0)*pow(glow*0.5, 2.2);
        col += vec3(0.25,0.4,0.9)*uTreble*0.35*glow;
      } else if (m == 10) {
        // warped checkerboard
        vec2 p = uv*(3.0+uMid*3.0);
        p *= mat2(cos(t),-sin(t),sin(t),cos(t));
        p += 0.2*sin(p.yx*4.0+t*2.0+uBass*6.0);
        float c = mod(floor(p.x)+floor(p.y), 2.0);
        col = hsv(fract(t*0.1+uTreble), 0.6, (0.2+0.8*c)*(0.4+uLevel+uBass*0.5));
      } else if (m == 11) {
        // spectrum analyser: a height per band, with peak caps riding on top
        float bands = 24.0;
        float bi = floor((uv.x+0.9)*bands/1.8);
        float bx = fract((uv.x+0.9)*bands/1.8);
        float f = bi/bands;                               // 0 = low .. 1 = high
        float band = mix(mix(uBass, uMid, smoothstep(0.0,0.55,f)),
                         uTreble, smoothstep(0.45,1.0,f));
        float h = band*(0.55+0.45*noise(vec2(bi*0.7, t*2.2))) + 0.03;
        float y = uv.y + 0.42;
        float inBar = step(0.12,bx)*step(bx,0.88)*step(0.0,y);
        float bar = inBar*step(y,h);
        float peak = inBar*smoothstep(0.02,0.0, abs(y-(h+0.03)));
        col = pal(fract(0.02 + y*0.55 + f*0.1),
                  vec3(0.5,0.35,0.25), vec3(0.5,0.4,0.3), vec3(1.0), vec3(0.0,0.12,0.25));
        col *= bar*(0.8+uLevel*0.8);
        col += vec3(0.9,0.5,0.2)*bar*smoothstep(h,0.0,y)*0.4;   // hotter at the base
        col += vec3(1.0,0.95,0.85)*peak*(0.7+uTreble);
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
        // nested polygons rushing outward, each ring counter-rotating
        float n = 3.0+floor(uMid*5.0);
        float ang = 6.2831/n;
        float acc = 0.0;
        for (int k=0;k<6;k++) {
          float fk = float(k);
          float scale = fract(t*0.22 + fk/6.0);
          float aa = a + t*(0.5-fk*0.12) + fk;
          float d = abs(cos(floor(0.5+aa/ang)*ang - aa)*r - scale*1.25);
          acc += (1.0-scale)*(0.004+uBass*0.010)/(d+0.006);
        }
        col = pal(fract(0.15 + t*0.05 + uTreble*0.2),
                  vec3(0.5), vec3(0.5), vec3(1.0,0.9,0.75), vec3(0.1,0.3,0.55));
        col *= acc*(0.7+uLevel*0.9);
      } else if (m == 15) {
        // oscilloscope: phosphor trace over a faint graticule
        float wave = 0.0;
        for (int k=1;k<=4;k++) {
          float fk = float(k);
          wave += sin(uv.x*(7.0*fk) + t*(3.0+fk))*(uBass/fk)*0.35;
          wave += sin(uv.x*(23.0*fk) - t*(5.0+fk))*(uTreble/fk)*0.12;
        }
        wave += sin(uv.x*13.0 + t*4.0)*uMid*0.18;
        float trace = (0.0035+uLevel*0.006)/(abs(uv.y - wave)+0.004);
        float grid = smoothstep(0.03,0.0, abs(fract(uv.x*5.0)-0.5))
                   + smoothstep(0.03,0.0, abs(fract(uv.y*5.0)-0.5));
        col = vec3(0.15,0.9,0.35)*trace;
        col += vec3(1.0)*pow(trace*0.6, 2.5);
        col += vec3(0.05,0.3,0.12)*grid*0.35;
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
      } else if (m == 19) {
        // mandala
        float seg = 8.0+floor(uMid*8.0);
        float aa = abs(mod(a, 6.2831/seg) - 3.1415/seg);
        float mand = sin(aa*10.0+t)*sin(r*15.0 - t*3.0 - uBass*8.0);
        col = hsv(fract(r+t*0.1+uTreble), 0.8, 0.5+0.5*mand+uLevel*0.4);
      } else if (m == 20) {
        // julia set, orbit-trapped — the seed drifts with bass and mids
        vec2 z = uv*1.6;
        vec2 c = vec2(0.36*cos(t*0.7+uBass*2.0), 0.36*sin(t*0.55+uMid*2.0));
        float trap = 1e9, esc = 0.0;
        for (int k=0;k<24;k++) {
          if (dot(z,z) < 16.0) {
            z = vec2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
            trap = min(trap, abs(length(z)-0.6));
            esc += 1.0;
          }
        }
        col = pal(fract(esc*0.03 + t*0.05 + uMid*0.3),
                  vec3(0.5), vec3(0.5), vec3(1.0,0.9,0.7), vec3(0.0,0.15,0.35));
        col *= exp(-trap*(6.0+uTreble*10.0))*(1.5+uBass*2.0) + 0.04;
      } else if (m == 21) {
        // nebula: domain-warped fbm clouds with a glowing core
        vec2 p = uv*1.4;
        p += 0.35*vec2(fbm(p*1.5 + t*0.4), fbm(p*1.5 - t*0.35 + 5.0));
        float dens = pow(fbm(p*2.0 + t*0.15)*1.3, 2.0) + 0.4*fbm(p*4.0 - t*0.22 + 3.0);
        col = pal(fract(0.62 + dens*0.5 + t*0.03 + uMid*0.2),
                  vec3(0.35,0.25,0.45), vec3(0.45,0.35,0.5), vec3(1.0), vec3(0.0,0.25,0.5));
        col *= dens*(1.2+uBass*1.5);
        col += vec3(0.5,0.6,1.0)*pow(max(0.0,1.0-r*1.2),3.0)*(0.15+uTreble*0.5);
      } else if (m == 22) {
        // wormhole: perspective depth with twisting ribs and distance fog
        float z = 1.0/(r+0.12);
        float ang = a + z*0.35 + t*0.6;
        float depth = z + t*(2.0+uBass*3.0);
        float rings = smoothstep(0.35,0.0, abs(fract(depth*0.5)-0.5));
        float ribs = 0.5+0.5*sin(ang*8.0);
        float fog = exp(-z*0.16);
        col = pal(fract(depth*0.05 + uMid*0.3), vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0,0.33,0.67));
        col *= (rings*0.8 + ribs*0.5 + sin(ang*3.0 + z*0.8 - t*2.0)*0.2)*fog*(1.0+uBass*1.5);
        col += vec3(1.0,0.7,0.4)*smoothstep(0.35,0.0,r)*(0.2+uBass*0.8);
      } else if (m == 23) {
        // warp speed: stars race outward along angular lanes
        float lanes = 42.0;
        float li = floor(a*lanes/6.2831 + 0.5);
        float seed = hash(vec2(li,7.0));
        float z = fract(seed + t*(0.25+uBass*0.9));
        float star = smoothstep(0.05,0.0, abs(a - li*6.2831/lanes)/(r+0.1))
                   * smoothstep(0.10*z+0.01,0.0, abs(r - z*z*1.4));
        col = pal(fract(seed+t*0.05), vec3(0.75), vec3(0.35), vec3(1.0), vec3(0.0,0.2,0.45));
        col *= star*(0.8+uLevel*1.2)*(0.4+z);
        col += vec3(0.6,0.8,1.0)*0.02/(r+0.03);
      } else if (m == 24) {
        // liquid: iterated domain warp, iridescent banding
        vec2 p = uv*1.8;
        for (int k=0;k<3;k++) {
          p += 0.5*vec2(sin(p.y*2.3 - t*1.1 + uBass*2.0), cos(p.x*2.1 + t*0.9));
          p *= 1.08;
        }
        float v = sin(p.x*1.5)*cos(p.y*1.5);
        col = pal(fract(v*0.5 + t*0.08 + uMid*0.3),
                  vec3(0.5), vec3(0.5), vec3(1.0,1.0,0.5), vec3(0.8,0.9,0.3));
        col *= 0.6+0.8*abs(v)+uLevel*0.5;
        col += vec3(0.2,0.35,0.6)*pow(abs(v),4.0)*(1.0+uTreble*2.0);
      } else if (m == 25) {
        // crystal: folded space, faceted edges
        vec2 p = uv*1.5;
        float rot = t*0.25 + uBass*0.6;
        p *= mat2(cos(rot),-sin(rot),sin(rot),cos(rot));
        for (int k=0;k<5;k++) {
          p = abs(p) - vec2(0.32+0.12*sin(t*0.5+uMid*2.0), 0.24);
          p *= mat2(cos(0.7),-sin(0.7),sin(0.7),cos(0.7))*1.18;
        }
        float d = abs(p.x)+abs(p.y);
        col = pal(fract(d*0.4 + t*0.06 + uTreble*0.25),
                  vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.15,0.35,0.6));
        col *= smoothstep(0.12,0.0,d)*(1.3+uBass*1.8) + 0.06/(d+0.08);
      } else if (m == 26) {
        // aurora: stacked curtains drifting on noise
        float acc = 0.0;
        for (int k=0;k<4;k++) {
          float fk = float(k);
          float wave = fbm(vec2(uv.x*1.6 + t*0.25 + fk*0.7, t*0.18 + fk*0.7))*0.9 - 0.35;
          acc += exp(-abs(uv.y - wave + fk*0.12 - 0.15)*(7.0 - uBass*3.0))*(0.5+0.5*sin(fk+t));
        }
        col = pal(fract(0.42 + uv.y*0.25 + t*0.03 + uMid*0.2),
                  vec3(0.25,0.5,0.4), vec3(0.3,0.45,0.35), vec3(1.0), vec3(0.0,0.25,0.5));
        col *= acc*(1.2+uBass*1.6);
        col += vec3(0.1,0.3,0.25)*acc*acc*uTreble*2.0;
      } else if (m == 27) {
        // vortex: filaments dragged into the core
        float swirl = a + 2.2/(r+0.25) - t*(1.0+uBass*1.6);
        float fil = pow(0.5+0.5*sin(swirl*(3.0+floor(uMid*5.0))), 3.0+uTreble*6.0);
        col = pal(fract(swirl*0.06 + t*0.05),
                  vec3(0.5), vec3(0.5), vec3(1.0,0.95,0.8), vec3(0.1,0.25,0.5));
        col *= fil*(0.8+uLevel*1.3) + smoothstep(0.5,0.0,r)*(0.35+uBass*0.9);
      } else if (m == 28) {
        // supernova: expanding shockwave over a ray burst
        float pulse = fract(t*0.35);
        float shock = smoothstep(0.06,0.0, abs(r - pulse*1.3))*(1.0-pulse);
        float rays = pow(0.5+0.5*sin(a*(14.0+floor(uTreble*14.0)) + t*0.6), 4.0);
        col = pal(fract(0.08 + r*0.4 + t*0.04),
                  vec3(0.6,0.4,0.3), vec3(0.5,0.35,0.25), vec3(1.0), vec3(0.0,0.15,0.3));
        col *= (0.05+uBass*0.10)/(r+0.05)
             + rays*smoothstep(1.1,0.1,r)*(0.4+uLevel)
             + shock*(2.0+uBass*3.0);
      } else if (m == 29) {
        // circuit: pulses race along the traces
        vec2 p = uv*(3.5+uMid*2.0);
        vec2 g = fract(p)-0.5;
        float rnd = hash(floor(p));
        float line = min(abs(g.x), abs(g.y));
        if (rnd > 0.5) line = min(line, abs(abs(g.x)-abs(g.y))*0.7071);
        float trace = smoothstep(0.055,0.0,line);
        float flow = fract((rnd > 0.5 ? p.x+p.y : p.x-p.y)*0.5 - t*(1.0+uBass*2.2) - rnd);
        float node = smoothstep(0.09,0.0,length(g))*(0.4+0.6*sin(t*2.0+rnd*6.28));
        col = pal(fract(0.45 + rnd*0.12 + t*0.02),
                  vec3(0.2,0.5,0.45), vec3(0.25,0.45,0.4), vec3(1.0), vec3(0.0,0.2,0.4));
        col *= trace*(0.35+uLevel*0.6) + trace*pow(1.0-flow,8.0)*(1.5+uBass*2.5) + node*(0.6+uTreble*1.5);
      } else if (m == 30) {
        // spectrum: classic analyser bars with peak caps
        float bars = 48.0;
        float bx = floor(sc.x*bars)/bars;
        float h = pow(specLog(bx + 0.5/bars), 0.75);
        float gap = smoothstep(0.06,0.14, fract(sc.x*bars));
        float bar = step(sc.y, h*0.92) * gap;
        float cap = smoothstep(0.02,0.0, abs(sc.y - h*0.92 - 0.02)) * gap;
        col = mix(vec3(0.1,0.9,0.25), vec3(1.0,0.75,0.1), sc.y/max(h,0.001));
        col = mix(col, vec3(1.0,0.2,0.15), smoothstep(0.6,0.95, sc.y));
        col *= bar;
        col += vec3(0.85,0.95,1.0)*cap;
      } else if (m == 31) {
        // waterfall: frequency across, time scrolling down
        float v = pow(specAt(pow(sc.x,1.8), 1.0 - sc.y), 0.65);
        // Explicit heat ramp. A cosine palette here collapsed into two flat
        // saturated blocks instead of the graded intensity a spectrogram needs.
        col = mix(vec3(0.02,0.01,0.07), vec3(0.24,0.06,0.5),  smoothstep(0.00,0.30,v));
        col = mix(col,                  vec3(0.00,0.62,0.9),  smoothstep(0.30,0.55,v));
        col = mix(col,                  vec3(1.0,0.85,0.15),  smoothstep(0.55,0.80,v));
        col = mix(col,                  vec3(1.0,1.0,1.0),    smoothstep(0.80,1.00,v));
      } else if (m == 32) {
        // vumeter: three analogue needles on lit dials
        float cell = floor(sc.x*3.0);
        vec2 p = vec2(fract(sc.x*3.0)-0.5, sc.y-0.12);
        p.x *= (iResolution.x/3.0)/iResolution.y;
        float lvl = cell < 0.5 ? uBass : (cell < 1.5 ? uMid : uTreble);
        float rr = length(p);
        // Mask by sweep angle: masking on p.y cut the dial in half rather than
        // ending it at the scale limits.
        float pa2 = atan(p.y, p.x);
        float inArc = step(0.52, pa2) * step(pa2, 2.62);
        float arc = smoothstep(0.014,0.0, abs(rr-0.34)) * inArc;
        float ticks = inArc * smoothstep(0.045,0.0, abs(rr-0.30)) * smoothstep(0.82,1.0, sin(pa2*24.0));
        float ang = mix(2.62, 0.52, clamp(lvl*1.25,0.0,1.0));
        vec2 dir = vec2(cos(ang), sin(ang));
        float needle = smoothstep(0.010,0.0, length(p - dir*clamp(dot(p,dir),0.0,0.32)));
        float hub = smoothstep(0.024,0.0, rr);
        col = vec3(0.9,0.82,0.55)*arc*0.55 + vec3(1.0,0.92,0.65)*ticks*0.5;
        col += mix(vec3(1.0,0.95,0.8), vec3(1.0,0.3,0.15), smoothstep(0.6,1.0,lvl))*needle;
        col += vec3(0.7,0.65,0.5)*hub;
        col += vec3(0.05,0.045,0.04);
      } else if (m == 33) {
        // scope: a trace built from the spectrum, drawn like a CRT
        float y = 0.0;
        for (int k=1;k<=6;k++){
          float fk = float(k);
          y += spec(fk/7.0) * sin(uv.x*(6.0+fk*7.0) + t*(2.0+fk)) / fk;
        }
        y *= 0.45;
        float d = abs(uv.y - y);
        col = vec3(0.25,1.0,0.45)*(smoothstep(0.045,0.0,d) + smoothstep(0.28,0.0,d)*0.25);
        col += vec3(0.1,0.35,0.15)*smoothstep(0.004,0.0,abs(uv.y))*0.5;
      } else if (m == 34) {
        // radial: the analyser wrapped around a circle
        float bars = 64.0;
        float ax = fract((a+3.14159)/6.28318);
        float bi = (floor(ax*bars)+0.5)/bars;
        float h = pow(specLog(abs(bi*2.0-1.0)), 0.6);
        float inner = 0.22 + uBass*0.04;
        float edge = inner + h*0.50;
        float gap = smoothstep(0.10,0.28, fract(ax*bars));
        float body = step(inner, r)*step(r, edge)*gap;
        float tip = smoothstep(0.018,0.0, abs(r-edge))*gap;
        col = pal(fract(bi + t*0.1), vec3(0.5,0.4,0.5), vec3(0.5,0.45,0.4), vec3(1.0), vec3(0.0,0.2,0.4));
        col *= body*0.7 + tip*2.0;
        col += vec3(0.5,0.7,1.0)*smoothstep(inner, inner*0.35, r)*(0.15+uBass*0.8);
      } else if (m == 35) {
        // ledladder: segmented level meters, the rack-gear look
        float cols = 16.0, rows = 14.0;
        vec2 g = vec2(floor(sc.x*cols), floor(sc.y*rows));
        vec2 f = fract(vec2(sc.x*cols, sc.y*rows));
        float h = specLog((g.x+0.5)/cols)*1.05;
        float lit = step((g.y+0.5)/rows, h);
        float seg = smoothstep(0.08,0.2,f.x)*smoothstep(0.08,0.2,f.y)
                  * smoothstep(0.08,0.2,1.0-f.x)*smoothstep(0.08,0.2,1.0-f.y);
        vec3 on = mix(vec3(0.15,1.0,0.3), vec3(1.0,0.85,0.1), (g.y+0.5)/rows);
        on = mix(on, vec3(1.0,0.2,0.1), smoothstep(0.72,1.0,(g.y+0.5)/rows));
        col = seg*(on*lit + vec3(0.05,0.06,0.07)*(1.0-lit));
      } else if (m == 36) {
        // terrain: the spectrum as a lit heightfield running to a horizon
        float horizon = 0.32;
        if (sc.y < horizon) {
          float depth = horizon - sc.y;
          float z = 0.06/(depth+0.02);
          float x = uv.x*z*0.55;
          float hgt = specLog(fract(x*0.12+0.5))*exp(-z*0.05);
          float grid = max(smoothstep(0.9,1.0,sin(x*6.0)), smoothstep(0.9,1.0,sin(z*2.0 - t*4.0)));
          col = pal(fract(0.6+hgt*0.4), vec3(0.3,0.3,0.5), vec3(0.35,0.3,0.45), vec3(1.0), vec3(0.0,0.2,0.45));
          col *= (0.15 + hgt*1.6 + grid*0.35)*exp(-z*0.04);
        } else {
          float sky = (sc.y-horizon)/(1.0-horizon);
          col = mix(vec3(0.06,0.03,0.12), vec3(0.02,0.01,0.05), sky);
          col += vec3(0.9,0.5,0.3)*exp(-sky*7.0)*(0.3+uBass);
          col += vec3(1.0)*step(0.997, hash(floor(sc*vec2(220.0,140.0))))*sky;
        }
      } else if (m == 37) {
        // strings: plucked wires, each tuned to a band
        float n = 9.0;
        for (int k=0;k<9;k++){
          float fk = float(k);
          float y0 = (fk+0.5)/n - 0.5;
          float amp = spec((fk+0.5)/n)*0.09;
          float d = abs(uv.y - (y0 + sin(uv.x*(8.0+fk*3.0) + t*(3.0+fk*0.7))*amp));
          col += pal(fract(fk/n*0.7+0.15), vec3(0.5), vec3(0.45), vec3(1.0), vec3(0.0,0.25,0.5))
               * (smoothstep(0.012,0.0,d) + smoothstep(0.06,0.0,d)*0.18) * (0.4+amp*9.0);
        }
      } else if (m == 38) {
        // matrix: falling glyph columns, speed set by the band beneath them
        float cols = 32.0;
        float cx = floor(sc.x*cols);
        float sp = 0.4 + specLog((cx+0.5)/cols)*2.2;
        float y = fract(sc.y*0.55 + t*sp + hash(vec2(cx,1.0)));
        float glyph = step(0.55, hash(floor(vec2(cx, sc.y*26.0 - t*sp*26.0))));
        col = vec3(0.15,1.0,0.35)*glyph*pow(1.0-y,3.0)*0.7
            + vec3(0.8,1.0,0.9)*glyph*smoothstep(0.12,0.0,y);
      } else if (m == 39) {
        // phyllo: sunflower spiral, seeds bloom with the spectrum
        for (int k=0;k<70;k++){
          float fk = float(k);
          float ang2 = fk*2.39996 + t*0.4;
          vec2 q = vec2(cos(ang2),sin(ang2))*sqrt(fk/70.0)*0.85;
          float e = spec(fk/70.0);
          col += pal(fract(fk/70.0*0.8+t*0.05), vec3(0.5),vec3(0.5),vec3(1.0),vec3(0.0,0.15,0.35))
               * smoothstep(0.035+e*0.03, 0.0, length(uv-q))*(0.25+e*1.6);
        }
      } else if (m == 40) {
        // voronoi: cells breathing with the level
        vec2 p = uv*(2.4+uMid*1.2);
        vec2 ip = floor(p);
        float d1 = 9.0, d2 = 9.0; vec2 best = vec2(0.0);
        for (int j=-1;j<=1;j++) for (int i=-1;i<=1;i++){
          vec2 g = ip+vec2(float(i),float(j));
          vec2 o = 0.5+0.45*sin(t*1.5+6.2831*vec2(hash(g), hash(g+7.7)));
          float d = length(g+o-p);
          if (d<d1){ d2=d1; d1=d; best=g; } else if (d<d2) d2=d;
        }
        float edge = smoothstep(0.0,0.09,d2-d1);
        float e = spec(fract(hash(best)));
        col = pal(fract(hash(best)*0.9+t*0.03), vec3(0.45,0.4,0.5), vec3(0.4), vec3(1.0), vec3(0.0,0.2,0.45));
        col *= (0.12+e*1.5)*edge + (1.0-edge)*0.9;
      } else if (m == 41) {
        // rain: drops on glass, impacts landing on the beat
        col = vec3(0.02,0.03,0.05);
        for (int k=0;k<14;k++){
          float fk = float(k);
          vec2 c = vec2(hash(vec2(fk,3.0))*2.0-1.0, hash(vec2(fk,9.0))*1.2-0.6);
          float ph = fract(t*(0.35+hash(vec2(fk,5.0))*0.5) + hash(vec2(fk,2.0)));
          float ring = smoothstep(0.03,0.0, abs(length(uv-c)-ph*(0.45+uBass*0.35)))*(1.0-ph);
          col += pal(fract(fk*0.13+0.55), vec3(0.3,0.4,0.5), vec3(0.3,0.35,0.4), vec3(1.0), vec3(0.0,0.2,0.4))*ring*1.4;
        }
      } else if (m == 42) {
        // fireworks: shells bursting on the low end
        for (int k=0;k<8;k++){
          float fk=float(k);
          float ph = fract(t*0.5 + hash(vec2(fk,1.7)));
          vec2 c = vec2(hash(vec2(fk,4.0))*1.6-0.8, hash(vec2(fk,8.0))*0.9-0.35);
          float shell = smoothstep(0.02,0.0, abs(length(uv-c)-ph*(0.5+uBass*0.5)))*pow(1.0-ph,2.0);
          float sparks = smoothstep(0.75,1.0, sin(atan(uv.y-c.y,uv.x-c.x)*18.0))*shell;
          col += hsv(fract(hash(vec2(fk,6.0))+t*0.05), 0.75, 1.0)*(shell*0.7+sparks*1.1);
        }
      } else if (m == 43) {
        // smoke: drifting fbm lit from the low end
        vec2 p = uv*1.6; p.y -= t*0.5;
        float d = fbm(p + fbm(p*1.7 + t*0.2)*0.9);
        col = pal(fract(0.62 + d*0.35 + uMid*0.15), vec3(0.35,0.3,0.4), vec3(0.4,0.35,0.4), vec3(1.0), vec3(0.0,0.2,0.45));
        col *= smoothstep(0.25,0.85,d)*(0.5+uLevel*1.3);
        col += vec3(1.0,0.55,0.25)*smoothstep(0.5,0.0,r)*uBass*0.7;
      } else if (m == 44) {
        // dna: twin helix, rungs lighting with the spectrum
        for (int k=0;k<26;k++){
          float fk=float(k);
          float y = (fk/26.0)*2.0-1.0;
          float ph = y*4.0 + t*2.2;
          float x1 = sin(ph)*0.42, x2 = sin(ph+3.14159)*0.42;
          float dep = 0.55+0.45*cos(ph);
          float e = spec(fk/26.0);
          float rad = 0.030 + 0.022*dep;
          col += vec3(0.3,0.8,1.0)*(smoothstep(rad,0.0,length(uv-vec2(x1,y)))
               + smoothstep(rad*2.6,0.0,length(uv-vec2(x1,y)))*0.25)*dep*(1.1+e*2.2);
          col += vec3(1.0,0.4,0.7)*(smoothstep(rad,0.0,length(uv-vec2(x2,y)))
               + smoothstep(rad*2.6,0.0,length(uv-vec2(x2,y)))*0.25)*dep*(1.1+e*2.2);
          vec2 pa = vec2(x1,y), pb = vec2(x2,y);
          vec2 ab = pb-pa; float h2 = clamp(dot(uv-pa,ab)/dot(ab,ab),0.0,1.0);
          col += vec3(0.6,0.9,0.7)*smoothstep(0.014,0.0,length(uv-pa-ab*h2))*(0.35+e*1.6);
        }
      } else if (m == 45) {
        // milkdrop: the frame feeds back into itself. Each pass samples the
        // previous one through a rotate+zoom warp and fades it, so motion
        // leaves trails that spiral inward. That inheritance IS the look —
        // it's what a single-pass shader can't fake.
        float ang3 = 0.05 + 0.10*sin(t*0.7) + uMid*0.12;
        float zoom = 0.982 - uBass*0.030;
        mat2 rot = mat2(cos(ang3), -sin(ang3), sin(ang3), cos(ang3));
        vec2 q = rot * uv * zoom;
        // ripple the sampling coordinate so trails bend rather than slide
        q += 0.012*vec2(sin(uv.y*7.0 + t*2.1), cos(uv.x*7.0 - t*1.7))*(0.4+uTreble);
        vec3 prev = texture2D(uPrev, vec2(q.x*iResolution.y/iResolution.x, q.y) + 0.5).rgb;
        // Decay has to outrun the ink or the buffer saturates to white within
        // seconds — feedback compounds, so these two numbers are the balance.
        prev *= 0.938 - 0.030*uTreble;
        prev -= 0.010;

        float lobes = 3.0 + floor(uMid*4.0);
        float petal = abs(sin(a*lobes + t*1.6)) * (0.34 + uBass*0.30);
        float shape = smoothstep(0.035, 0.0, abs(r - petal));
        float core  = smoothstep(0.07 + uBass*0.06, 0.0, r);
        float sparkle = smoothstep(0.988, 1.0, hash(floor(uv*90.0 + t*30.0))) * uTreble;
        // Angle in the palette lookup keeps hue moving around the frame instead
        // of washing everything with a single colour.
        vec3 ink = pal(fract(t*0.13 + r*0.55 + a*0.16 + uMid*0.2),
                       vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0,0.33,0.67));
        float reach = smoothstep(1.05, 0.15, r);
        col = max(prev, vec3(0.0));
        col += ink * (shape*(0.30+uLevel*0.45) + core*(0.12+uBass*0.30) + sparkle*0.35) * reach;
        col += vec3(0.6,0.8,1.0) * spec(fract(a/6.28318+0.5)) * smoothstep(0.8,0.2,r) * 0.05;
      } else if (m == 46) {
        // ripple: interfering wave sources
        float v = 0.0;
        for (int k=0;k<5;k++){
          float fk=float(k);
          vec2 c = vec2(cos(fk*1.4+t*0.7), sin(fk*2.1+t*0.5))*0.55;
          v += sin(length(uv-c)*(22.0+spec(fk/5.0)*40.0) - t*5.0);
        }
        v /= 5.0;
        col = pal(fract(0.5+v*0.35+t*0.03), vec3(0.4,0.45,0.55), vec3(0.4), vec3(1.0), vec3(0.0,0.2,0.4));
        col *= 0.35+0.9*abs(v)+uLevel*0.5;
      } else if (m == 47) {
        // comets: streaks orbiting a core
        for (int k=0;k<12;k++){
          float fk=float(k);
          float ang2 = t*(0.5+hash(vec2(fk,2.0))*1.2)*2.0 + fk*0.52;
          float rad = 0.25+hash(vec2(fk,4.0))*0.55;
          vec2 c = vec2(cos(ang2),sin(ang2))*rad;
          float d = length(uv-c);
          // Sample back along the orbit so each comet drags a real tail rather
          // than a blob smeared towards the centre.
          float tail = 0.0;
          for (int s=1;s<=6;s++){
            float fs=float(s);
            vec2 cp = vec2(cos(ang2-fs*0.075),sin(ang2-fs*0.075))*rad;
            tail += smoothstep(0.030,0.0,length(uv-cp))*(1.0-fs/7.0)*0.5;
          }
          col += hsv(fract(fk*0.08+t*0.05),0.7,1.0)
               * (smoothstep(0.038,0.0,d)*(1.2+spec(fk/12.0)*2.4)
                + smoothstep(0.11,0.0,d)*0.35 + tail);
        }
        col += vec3(0.8,0.85,1.0)*smoothstep(0.12,0.0,r)*(0.4+uBass);
      } else if (m == 48) {
        // ribbon: a wide band twisting through the frame
        float w = 0.0;
        for (int k=1;k<=4;k++){
          float fk=float(k);
          w += sin(uv.x*(2.0+fk*2.5) + t*(1.0+fk*0.4))*spec(fk/5.0)*0.35/fk;
        }
        float d = abs(uv.y - w);
        float thick = 0.06+uLevel*0.10;
        col = pal(fract(uv.x*0.35+t*0.08), vec3(0.5,0.45,0.5), vec3(0.45), vec3(1.0), vec3(0.0,0.2,0.45));
        col *= smoothstep(thick,thick*0.2,d)*0.75 + smoothstep(0.012,0.0, abs(d-thick))*1.7;
      } else {
        // glitch: the spectrum torn into shifted scanline blocks
        float band = floor(sc.y*22.0);
        float sh = (hash(vec2(band, floor(t*7.0)))-0.5)*specLog(band/22.0)*0.7;
        vec2 q = vec2(sc.x + sh, sc.y);
        float scan = 0.65+0.35*sin(sc.y*iResolution.y*0.7);
        col = vec3(specLog(fract(q.x)));
        col.r = specLog(fract(q.x+0.012));
        col.b = specLog(fract(q.x-0.012));
        col = mix(col*vec3(0.3,0.9,0.6), col, 0.55)*scan*1.5;
        col += step(0.995, hash(vec2(floor(sc.x*90.0), floor(t*20.0))))*0.35;
      }

      // Modes that own their whole output. Panel displays (30,31,32,35,38,49)
      // fill the frame, so the radial vignette would eat their corners and the
      // level scaling would dim the readings they exist to show.
      //
      // milkdrop (45) must opt out for a different and stricter reason: it
      // samples its own previous output, so any post-process would be reapplied
      // every frame and compound — the trails would be scrubbed out within a
      // few frames instead of decaying the way the shader intends.
      bool selfLit = (m==30)||(m==31)||(m==32)||(m==35)||(m==38)||(m==45)||(m==49);
      if (!selfLit) {
        col *= 0.35+0.9*uLevel+0.3*uBass;
        col *= smoothstep(1.5, 0.15, r);
      }

      // Trail layer, applied AFTER the post-process on purpose: the vignette
      // and level scaling must land on the fresh frame only. Blending first
      // would re-apply them to inherited pixels every frame, compounding until
      // the trails were scrubbed away.
      float fb = feedbackFor(m);
      if (fb > 0.0) {
        // The swirl is seeded from the mode index so every pattern drifts its
        // own way instead of all fifty sharing one motion.
        float fa = 0.03 + 0.05*sin(t*0.6 + float(m)) + uMid*0.08;
        float fz = 0.988 - uBass*0.020;
        mat2 frot = mat2(cos(fa), -sin(fa), sin(fa), cos(fa));
        vec2 fq = frot * uv * fz;
        vec3 prev = texture2D(uPrev, vec2(fq.x*iResolution.y/iResolution.x, fq.y) + 0.5).rgb;
        prev = prev*(fb - 0.02*uTreble) - 0.006;
        // Trails obey the frame too. Without this they drift outward past the
        // vignette that was applied to the fresh pixels and light up the edges,
        // and the window loses its framed look entirely.
        prev *= smoothstep(1.6, 0.2, r);
        // max() keeps each mode at its own brightness; adding would stack the
        // inherited frame on top of the new one and saturate to white.
        col = max(col, max(prev, vec3(0.0)));
      }
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

  // Free Mode feeds the visualizer from the system-audio loopback instead of
  // the librespot sink; resolved once at mount.
  let spectrumCommand = "take_latest_spectrum";

  onMount(() => {
    REACTIVE_WINDOW_SIZE.setSize(320, 240);
    REACTIVE_WINDOW_SIZE.setZoom(1);
    invoke("is_controller_mode")
      .then((on) => {
        if (on) {
          spectrumCommand = "loopback_spectrum";
          invoke("start_loopback").catch(() => {});
        }
      })
      .catch(() => {});

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

    // Spectrum texture: 256 bins across, 64 frames of history down (row 0 is
    // the newest). The four smoothed bands can't express what an analyser or a
    // spectrogram has to show, so the raw spectrum goes to the shader as well.
    // LUMINANCE/UNSIGNED_BYTE keeps it to a 16 KB upload with no float-texture
    // extension needed.
    const SPEC_W = 256,
      SPEC_H = 64;
    const specData = new Uint8Array(SPEC_W * SPEC_H);
    let specDirty = true;
    const specTex = gl.createTexture();
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, specTex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.uniform1i(u("uSpec"), 0);

    // Previous-frame texture for the feedback mode. Filled by copying the
    // backbuffer straight after the draw, which avoids framebuffer ping-pong
    // entirely — and the copy only runs while that mode is on screen, so every
    // other mode keeps exactly the render path it had before.
    // Must mirror feedbackFor() in the shader: these read the previous frame,
    // so the backbuffer has to be copied back after drawing them. The
    // instrument displays deliberately don't, and skipping the copy keeps them
    // on the exact render path they had before feedback existed.
    const NO_FEEDBACK = new Set(["spectrum", "waterfall", "vumeter", "ledladder", "matrix", "glitch"]);
    const needsPrevFrame = (/** @type {number} */ i) => !NO_FEEDBACK.has(MODE_NAMES[i]);
    const prevTex = gl.createTexture();
    gl.activeTexture(gl.TEXTURE1);
    gl.bindTexture(gl.TEXTURE_2D, prevTex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    // One black pixel so the sampler is complete on the very first frame.
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB, 1, 1, 0, gl.RGB, gl.UNSIGNED_BYTE, new Uint8Array([0, 0, 0]));
    gl.uniform1i(u("uPrev"), 1);
    gl.activeTexture(gl.TEXTURE0);

    /** Scroll the history down a row and write `row` into row 0. */
    const pushSpectrumRow = (/** @type {(x: number) => number} */ row) => {
      specData.copyWithin(SPEC_W, 0, SPEC_W * (SPEC_H - 1));
      for (let x = 0; x < SPEC_W; x++) specData[x] = row(x) * 255;
      specDirty = true;
    };

    let bass = 0,
      mid = 0,
      treble = 0,
      level = 0,
      running = true;
    const start = performance.now();

    let pollTimer = setTimeout(function poll() {
      if (!running) return;
      invoke(spectrumCommand, {})
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
            pushSpectrumRow((x) => v[Math.min(n - 1, Math.floor((x / SPEC_W) * n))]);
          } else {
            bass *= 0.94;
            mid *= 0.94;
            treble *= 0.94;
            level *= 0.94;
            // Keep the history scrolling into silence rather than freezing the
            // last frame across the spectrogram.
            pushSpectrumRow(() => 0);
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
      if (specDirty) {
        gl.bindTexture(gl.TEXTURE_2D, specTex);
        gl.texImage2D(
          gl.TEXTURE_2D, 0, gl.LUMINANCE, SPEC_W, SPEC_H, 0,
          gl.LUMINANCE, gl.UNSIGNED_BYTE, specData,
        );
        specDirty = false;
      }
      gl.drawArrays(gl.TRIANGLES, 0, 6);
      if (needsPrevFrame(mode)) {
        // Grab what was just drawn, before the browser composites it away, so
        // the next pass can inherit it.
        gl.activeTexture(gl.TEXTURE1);
        gl.bindTexture(gl.TEXTURE_2D, prevTex);
        gl.copyTexImage2D(
          gl.TEXTURE_2D, 0, gl.RGB, 0, 0,
          gl.drawingBufferWidth, gl.drawingBufferHeight, 0,
        );
        gl.activeTexture(gl.TEXTURE0);
      }
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
    makeDockedDraggable(element, "visualizer", "visualizerWindow");
  }

  function makeVizResizable(element) {
    makeSnappingResizer(
      element,
      "visualizer",
      (e) => {
        const zoom = REACTIVE_WINDOW_SIZE.zoom || 1;
        return {
          width: Math.max(Math.round(e.clientX / zoom) + 3, 180),
          height: Math.max(Math.round(e.clientY / zoom) + 3, 140),
        };
      },
      ({ width, height }) => REACTIVE_WINDOW_SIZE.setSize(Math.round(width), Math.round(height)),
      () => REACTIVE_WINDOW_SIZE.zoom || 1,
    );
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
    /* Same plugin-window rule the library follows: GENEX.BMP palette when a
       .wsz is loaded, PLEDIT as fallback. It used to be plain #000, which is
       also the canvas colour — the two merged and the window read as having no
       sides or bottom at all. */
    /* Frame follows the skin: the border and bevel derive from the window
       colour instead of a fixed blue, so on a gold or grey skin the edges no
       longer clash with the titlebar. */
    --frame: var(--skin-titlebarcolor, var(--skin-genexwndbg, var(--skin-plbg, #1a1a2a)));
    background: var(--frame);
    border: 1px solid color-mix(in srgb, var(--frame) 45%, #000);
    box-shadow:
      inset 1px 1px 0 color-mix(in srgb, var(--frame) 65%, #fff),
      inset -1px -1px 0 color-mix(in srgb, var(--frame) 55%, #000);
    user-select: none;
  }

  .viz-titlebar {
    position: relative;
    flex: 0 0 20px;
    height: 20px;
    background: var(--skin-genfill)
      repeat-x;
    cursor: default;
  }
  .viz-tl {
    position: absolute;
    left: 0;
    top: 0;
    width: 25px;
    height: 20px;
    background: var(--skin-gentl)
      no-repeat;
  }
  .viz-title {
    position: absolute;
    left: 50%;
    top: 0;
    transform: translateX(-50%);
    /* Full titlebar height so the plate tile lines up with the bars 1:1. */
    height: 20px;
    /* Centre the text, then lift it clear of the titlebar's thin inner-frame
       line along the bottom dead centre looks low because that line eats the
       lower edge. box-sizing keeps the padding inside the 20px (the bug the
       plain-centre version had). The bottom padding is the knob: increase it to
       raise the text, decrease it to lower it. */
    display: flex;
    align-items: center;
    justify-content: center;
    box-sizing: border-box;
    line-height: 1;
    padding: 0 10px 5px;
    font-family: "Segoe UI", Tahoma, sans-serif;
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 2px;
    color: var(--skin-titletext, var(--skin-genexhdrtext, #cdd6ea));
    /* The skin's own title plate, located in GEN.BMP by scanning (see
       wsz.rs::locate_title_plate) and served as --skin-gentitle. A classic
       striped titlebar sets it to the real plate slice, so the bars stop, the
       title sits on the plate, and the bars resume — the main-window look. A
       smooth titlebar sets it to `transparent`, so the actual titlebar shows
       through behind the title instead of a mismatched box. The shadow keeps
       the text legible in the transparent case. */
    background: var(--skin-gentitle, transparent) repeat-x;
    text-shadow: 0 1px 0 rgba(0, 0, 0, 0.55);
    z-index: 1;
  }
  .viz-close {
    position: absolute;
    right: 0;
    top: 0;
    width: 15px;
    height: 20px;
    background: var(--skin-gentr)
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
    /* Inset the canvas so the window background reads as a frame on three
       sides, the way the playlist and library windows do, and bevel it inward
       so the display looks recessed rather than pasted on. */
    margin: 0 4px 4px;
    border: 1px solid #0c0d12;
    box-shadow: inset 1px 1px 0 #0e0f16, inset -1px -1px 0 #3a3f52;
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
