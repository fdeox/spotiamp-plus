<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { REACTIVE_WINDOW_SIZE } from "$lib/common.svelte.js";
  import { subscribeToWindowEvent } from "$lib/events.svelte.js";
  import { makeDockedDraggable } from "$lib/window-docking.svelte.js";

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
    // Cosine gradient palette. Gives coherent, art-directed colour ramps
    // instead of the full-rainbow sweep hsv() produces.
    vec3 pal(float x, vec3 a, vec3 b, vec3 c, vec3 d){ return a + b*cos(6.28318*(c*x+d)); }

    void main() {
      vec2 uv = (gl_FragCoord.xy - 0.5*iResolution)/iResolution.y;
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
      } else {
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
    makeDockedDraggable(element, "visualizer", "visualizerWindow");
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
