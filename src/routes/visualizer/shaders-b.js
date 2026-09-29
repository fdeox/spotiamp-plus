// The second bank of visualizer patterns (51-100). A separate program from the
// first fifty on purpose: one shader holding all hundred would take twice as
// long to compile (the first fifty already take ~2.5 s through ANGLE), and
// keeping them apart leaves the original patterns exactly as they were.
// Same uniforms, helpers and post-processing as the first bank.
import { SHADER_COMMON } from "./shader-common.js";

export const MODE_NAMES_B = [
  "synthwave",
  "skyline",
  "ridgelines",
  "vinyl",
  "julia",
  "sierpinski",
  "truchet",
  "hypnotic",
  "fire",
  "ocean",
  "clouds",
  "bubbles",
  "snow",
  "confetti",
  "dotmatrix",
  "mirrorbars",
  "blobring",
  "interference",
  "zoomsquares",
  "argyle",
  "rose",
  "sonar",
  "heartbeat",
  "trimeter",
  "plasmaglobe",
  "galaxy",
  "caustics",
  "lavalamp",
  "tileflip",
  "zebra",
  "chroma",
  "blackhole",
  "sunrays",
  "cybergrid",
  "halftone",
  "spirograph",
  "starpoly",
  "pixelrain",
  "circlegram",
  "eye",
  "tessellate",
  "flower",
  "orbits",
  "wirecube",
  "orb",
  "pendulum",
  "dunes",
  "prism",
  "tenprint",
  "crt",
];

// The patterns that inherit the previous frame (trails). Must match
// feedbackForB() in the shader: the page only copies the frame back for these.
export const FEEDBACK_B = new Set([
  "blobring",
  "zoomsquares",
  "rose",
  "sonar",
  "plasmaglobe",
  "spirograph",
  "starpoly",
  "flower",
  "orbits",
  "wirecube",
  "pendulum",
]);

export const FRAG_B = SHADER_COMMON.slice(1) + `
    float sdSeg(vec2 p, vec2 a, vec2 b){
      vec2 pa = p-a, ba = b-a;
      float h = clamp(dot(pa,ba)/dot(ba,ba), 0.0, 1.0);
      return length(pa - ba*h);
    }
    // One heartbeat: P wave, the QRS spike, T wave.
    float ecgShape(float x){
      float ph = fract(x*3.0);
      return exp(-pow((ph-0.30)*45.0,2.0)) - 0.35*exp(-pow((ph-0.335)*45.0,2.0))
           + 0.18*exp(-pow((ph-0.55)*12.0,2.0)) + 0.08*exp(-pow((ph-0.18)*20.0,2.0));
    }
    // A sphere with a moving bumpy skin, pushed out by the bass.
    float orbSDF(vec3 p){
      float disp = sin(p.x*5.0 + iTime*1.3)*sin(p.y*5.0 + iTime*1.1)*sin(p.z*5.0 + iTime*0.9);
      return length(p) - 0.8 - disp*(0.04 + uBass*0.11);
    }

    float feedbackForB(int m) {
      if (m==35) return 0.92;                      // spirograph builds its figure up
      if (m==21 || m==42) return 0.90;             // sonar phosphor, orbit trails
      if (m==16) return 0.88;
      if (m==18 || m==20 || m==36 || m==41 || m==43 || m==45) return 0.85;
      if (m==24) return 0.80;
      return 0.0;
    }

    void main() {
      vec2 uv = (gl_FragCoord.xy - 0.5*iResolution)/iResolution.y;
      vec2 sc = gl_FragCoord.xy / iResolution;
      float t = iTime*0.25;
      float r = length(uv);
      float a = atan(uv.y, uv.x);
      float asp = iResolution.x/iResolution.y;
      int m = int(uMode + 0.5);
      vec3 col = vec3(0.0);

      if (m == 0) {
        // synthwave: a striped sun over a scrolling neon grid, spectrum mountains
        float hz = -0.08;
        col = mix(vec3(0.05,0.0,0.12), vec3(0.55,0.08,0.45), smoothstep(0.5,hz,uv.y));
        vec2 sp = uv - vec2(0.0, 0.12);
        float sr = 0.26 + uBass*0.03;
        float sun = smoothstep(sr, sr-0.01, length(sp));
        float gap = step(0.0, sin(sp.y*70.0 - iTime*2.0) + 1.0 + sp.y*9.0);
        sun *= sp.y > -0.02 ? 1.0 : gap;
        col = mix(col, mix(vec3(1.0,0.85,0.25), vec3(1.0,0.2,0.55), smoothstep(0.26,-0.2,sp.y)), sun);
        col += vec3(1.0,0.3,0.6)*0.3*exp(-length(sp)*4.0);
        float mh = hz + 0.02 + specLog(abs(sc.x-0.5)*1.7)*0.2;
        if (uv.y < mh && uv.y > hz) col = mix(vec3(0.06,0.0,0.12), vec3(0.28,0.05,0.38), (uv.y-hz)/(mh-hz+0.001));
        col += vec3(0.3,0.9,1.0)*smoothstep(0.004,0.0,abs(uv.y-mh))*step(hz,uv.y);
        if (uv.y < hz) {
          float d = hz - uv.y;
          float z = 0.12/d;
          vec2 g = vec2(uv.x*z + 0.5, z + iTime*(1.2+uBass*2.5));
          vec2 f = abs(fract(g)-0.5);
          float lw = clamp(0.035*z, 0.03, 0.45);
          float line = smoothstep(0.5-lw, 0.5, max(f.x, f.y));
          col = vec3(0.04,0.0,0.08) + vec3(1.0,0.25,0.85)*line*(0.7+uMid)*exp(-z*0.09);
          col += vec3(0.8,0.2,0.6)*exp(-d*22.0)*0.6;
        }
      } else if (m == 1) {
        // skyline: a night city whose towers are the spectrum, windows flickering
        col = mix(vec3(0.02,0.02,0.07), vec3(0.16,0.06,0.26), sc.y);
        col += step(0.996, hash(floor(gl_FragCoord.xy*0.5)))*0.7*step(0.4, sc.y);
        col += vec3(1.0,0.95,0.8)*smoothstep(0.055,0.05,length(uv-vec2(asp*0.3,0.3)));
        float n = 22.0;
        float bi = floor(sc.x*n);
        float bx = fract(sc.x*n);
        float h = 0.12 + specLog((bi+0.5)/n)*0.62 + hash(vec2(bi,3.0))*0.12;
        if (sc.y < h && bx > 0.06) {
          col = vec3(0.03,0.03,0.07);
          vec2 wc = vec2(bx*4.0, sc.y*55.0);
          vec2 cell = floor(wc);
          vec2 inCell = fract(wc);
          float lit = step(0.62 - uBass*0.3, hash(cell + bi*13.1 + floor(iTime*0.4 + hash(cell+bi)*9.0)));
          float win = step(0.2,inCell.x)*step(inCell.x,0.8)*step(0.25,inCell.y)*step(inCell.y,0.75)*step(0.5, cell.x)*step(cell.x, 2.5);
          col += win*lit*mix(vec3(1.0,0.8,0.4), vec3(0.45,0.8,1.0), hash(cell+bi))*(0.55+uMid*0.8);
          col += vec3(0.9,0.3,0.8)*smoothstep(0.006,0.0,abs(sc.y-h))*(0.4+uTreble);
        }
      } else if (m == 2) {
        // ridgelines: the spectrum's last two seconds as stacked ridges, each in
        // front hiding the ones behind, newest at the bottom
        float x = (sc.x-0.2)/0.6;
        if (x > 0.0 && x < 1.0) {
          float fq = pow(abs(x-0.5)*2.0, 1.5)*0.6;
          float env = smoothstep(0.0,0.3,x)*smoothstep(1.0,0.7,x);
          float cover = 0.0;
          for (int k = 0; k < 26; k++) {
            float fk = float(k);
            float y = 0.12 + fk*0.028 + specAt(fq, fk*2.0/63.0)*env*0.2
                    + (hash(vec2(floor(x*60.0), fk))-0.5)*0.002*env;
            col += vec3(1.0)*(1.0-cover)*smoothstep(0.0035, 0.0, abs(sc.y-y));
            if (sc.y < y) cover = 1.0;
          }
        }
      } else if (m == 3) {
        // vinyl: a spinning record, grooves catching the light, label on the bass
        float la = atan(uv.y, uv.x);
        float ang = la + iTime*3.5;
        col = vec3(0.015);
        if (r < 0.46) {
          float groove = 0.5+0.5*sin(r*420.0);
          float shine = pow(abs(cos(la - 0.7)), 26.0) + 0.5*pow(abs(cos(la + 1.1)), 40.0);
          col = vec3(0.035) + vec3(0.2,0.22,0.28)*shine*(0.55+groove*0.8)*(0.6+uTreble*1.2);
          col *= 0.8 + 0.2*step(0.03, abs(fract(r*9.0)-0.5));
          col += vec3(0.4)*smoothstep(0.004,0.0,abs(r-0.455));
          if (r < 0.15) {
            col = pal(0.1+uMid*0.3, vec3(0.6,0.3,0.3), vec3(0.4,0.3,0.2), vec3(1.0), vec3(0.0,0.1,0.2))*(0.7+uBass*0.7);
            col *= 0.75 + 0.25*step(0.5, fract(ang/6.2831*2.0));
            col *= 1.0 - 0.6*smoothstep(0.003,0.0,abs(r-0.1));
            if (r < 0.012) col = vec3(0.0);
          }
        }
      } else if (m == 4) {
        // julia: a Julia set whose constant orbits with the music
        vec2 z = uv*(2.4 - uBass*0.5);
        vec2 c = vec2(-0.745, 0.11) + vec2(0.09*cos(t*0.9 + uMid*2.0), 0.07*sin(t*0.7));
        float it = 0.0;
        for (int k=0;k<48;k++){
          z = vec2(z.x*z.x - z.y*z.y, 2.0*z.x*z.y) + c;
          if (dot(z,z) > 16.0) break;
          it += 1.0;
        }
        float v = (it - log2(max(1.0, log2(max(dot(z,z),1.0)))))/48.0;
        col = it > 47.5 ? vec3(0.0) : pal(v*2.5 + t*0.15, vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0,0.1,0.2))*pow(clamp(v,0.0,1.0),0.45)*1.3;
      } else if (m == 5) {
        // sierpinski: the triangle of triangles, turning, holes coloured by depth
        float ro = t*0.8;
        vec2 q = mat2(cos(ro),-sin(ro),sin(ro),cos(ro))*uv*(1.25 - uBass*0.2);
        vec2 d0 = q - vec2(-0.5,-0.289);
        vec2 s = vec2(0.0, d0.y/0.866);
        s.x = d0.x - 0.5*s.y;
        float inTri = step(0.0,s.x)*step(0.0,s.y)*step(s.x+s.y,1.0);
        vec2 w = s;
        float depth = 8.0;
        for (int k=0;k<7;k++){
          w *= 2.0;
          vec2 b = mod(floor(w), 2.0);
          if (b.x > 0.5 && b.y > 0.5) { depth = float(k); break; }
        }
        col = depth > 7.5
          ? hsv(fract(t*0.2 + s.x*0.3 + s.y*0.2), 0.6, 1.0)
          : hsv(fract(depth*0.13 + t*0.3), 0.8, 0.12 + spec(depth/7.0)*0.5);
        col *= inTri;
      } else if (m == 6) {
        // truchet: quarter-circle tiles, each flipping over now and then
        vec2 p = uv*(5.0+uMid*2.0) + vec2(t*0.8, t*0.3);
        vec2 id = floor(p);
        vec2 f = fract(p)-0.5;
        if (hash(id + floor(iTime*0.6 + hash(id)*10.0)) > 0.5) f.x = -f.x;
        float d = min(abs(length(f-0.5)-0.5), abs(length(f+0.5)-0.5));
        float w = 0.07 + uBass*0.07;
        float line = smoothstep(w, w*0.4, d);
        col = pal(fract(dot(id,vec2(0.07,0.11)) + t*0.1), vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.3,0.2,0.2))*line;
        col += line*pow(max(1.0-d/w,0.0),4.0)*0.4*uTreble;
      } else if (m == 7) {
        // hypnotic: a black and white spiral, colour bleeding in on the bass
        float s = step(0.5, fract(log(r+0.001)*1.8 + a/6.2831*3.0 - iTime*(0.5+uBass)));
        vec3 c2 = hsv(fract(t*0.3), 0.9, 1.0);
        col = s*mix(vec3(1.0), c2, clamp(uBass*1.3, 0.0, 1.0));
        col *= smoothstep(0.0, 0.08, r);
      } else if (m == 8) {
        // fire: flames licking up from the bottom, taller when it's loud
        vec2 p = vec2(uv.x*2.2, sc.y);
        float n = fbm(vec2(p.x*2.0, p.y*3.0 - iTime*1.6)) + 0.5*fbm(vec2(p.x*4.0+3.0, p.y*6.0 - iTime*2.6));
        float h = 0.3 + uLevel*0.55 + uBass*0.25;
        float f = clamp(n*1.3 - sc.y/h, 0.0, 1.0);
        col = vec3(1.6*f, 1.6*f*f*f, pow(f,6.0));
        col = pow(col, vec3(0.8))*1.25;
      } else if (m == 9) {
        // ocean: moonlit waves rolling in, the swell on the bass
        float hz = 0.1;
        vec2 moon = vec2(0.25*asp, 0.3);
        if (uv.y > hz) {
          col = mix(vec3(0.08,0.1,0.2), vec3(0.02,0.03,0.08), smoothstep(hz,0.5,uv.y));
          col += vec3(0.9,0.9,0.8)*smoothstep(0.065,0.06,length(uv-moon));
          col += vec3(0.3,0.35,0.4)*0.05/(length(uv-moon)+0.05);
          col += step(0.997, hash(floor(gl_FragCoord.xy*0.5)))*0.6*smoothstep(hz+0.05,0.5,uv.y);
        } else {
          // water-plane coordinates: far (small d) is compressed near the horizon
          float d = hz - uv.y;
          float z = 1.0/(d*6.0 + 0.05);
          vec2 wp = vec2(uv.x*z*2.0, z*1.5);
          // ripples stretched sideways, the way distant water reads
          float n = noise(wp*vec2(3.0, 6.0) + vec2(0.0, iTime*0.8))*0.6
                  + noise(wp*vec2(7.0, 13.0) - vec2(iTime*0.5, iTime*1.2))*0.4;
          float swell = 0.5 + 0.5*sin(wp.y*2.0 - iTime*1.4);
          col = vec3(0.01,0.03,0.07) + vec3(0.02,0.06,0.12)*(n*0.8 + swell*0.4)*(0.6 + uBass);
          // the moon's glitter: a column of short bright streaks, widening
          // towards you, sparkling harder with the treble
          float column = exp(-pow((uv.x - moon.x)/(0.03 + d*0.25), 2.0));
          col += vec3(0.95,0.95,0.8)*column*smoothstep(0.62 - uTreble*0.15, 0.85, n)*(0.8 + uLevel);
          col = mix(col, vec3(0.08,0.1,0.2), exp(-d*40.0));
        }
      } else if (m == 10) {
        // clouds: sunset cloud layers drifting past, lit from below by the bass
        col = mix(vec3(1.0,0.55,0.25), vec3(0.25,0.15,0.45), smoothstep(-0.4,0.5,uv.y));
        for (int k=0;k<3;k++){
          float fk = float(k);
          vec2 p = uv*(1.5+fk*0.8) + vec2(iTime*(0.05+fk*0.04), fk*3.1);
          float c = smoothstep(0.45, 0.8, fbm(p*2.0) + uMid*0.15);
          vec3 cc = mix(vec3(0.35,0.2,0.35), vec3(1.0,0.7,0.5), clamp(0.5 - uv.y + uBass*0.5, 0.0, 1.0));
          col = mix(col, cc*(0.7+fk*0.15), c*0.8);
        }
      } else if (m == 11) {
        // bubbles: rising through dark water, bigger and faster when it's loud
        col = mix(vec3(0.0,0.04,0.09), vec3(0.0,0.14,0.24), sc.y);
        for (int k=0;k<3;k++){
          float fk = float(k);
          vec2 p = uv*(6.0 + fk*4.0) - vec2(0.0, iTime*(0.6+fk*0.3)*(1.0+uBass));
          vec2 id = floor(p);
          vec2 f = fract(p)-0.5;
          float h = hash(id + fk*11.0);
          vec2 o = vec2(hash(id+3.1)-0.5, hash(id+7.7)-0.5)*0.3 + vec2(sin(iTime*2.0+h*6.0)*0.07, 0.0);
          float rad = (0.1 + 0.16*h)*(0.8 + uLevel*0.6);
          float d = length(f - o);
          float ring = smoothstep(0.035, 0.0, abs(d-rad))
                     + smoothstep(rad*0.45, 0.0, length(f - o - vec2(-rad*0.35, rad*0.35)))*0.8;
          col += step(0.45, h)*ring*vec3(0.5,0.85,1.0)*(0.5+0.25*fk);
        }
      } else if (m == 12) {
        // snow: flakes in parallax layers, the wind picking up with the treble
        col = mix(vec3(0.12,0.14,0.25), vec3(0.02,0.03,0.08), sc.y);
        for (int k=0;k<4;k++){
          float fk = float(k);
          vec2 p = uv*(8.0 + fk*6.0);
          p.y += iTime*(0.8 + fk*0.4);
          p.x += iTime*(0.2 + uTreble*1.5)*(1.0 + fk*0.3) + sin(p.y*0.5 + fk)*0.5;
          vec2 id = floor(p);
          vec2 f = fract(p)-0.5;
          vec2 o = (vec2(hash(id+fk), hash(id+fk*2.3))-0.5)*0.6;
          float flake = smoothstep(0.09/(1.0+fk*0.25), 0.0, length(f-o))*step(0.35, hash(id+9.1+fk));
          col += flake*(0.95 - fk*0.15)*(0.6 + uLevel*0.8);
        }
      } else if (m == 13) {
        // confetti: tumbling paper squares in party colours
        for (int k=0;k<3;k++){
          float fk = float(k);
          vec2 p = uv*(5.0 + fk*2.5);
          p.y += iTime*(1.0 + fk*0.4)*(0.7 + uBass*0.8);
          vec2 id = floor(p);
          vec2 f = fract(p)-0.5;
          float h = hash(id + fk*5.0);
          float an = iTime*(2.0 + h*4.0) + h*6.28;
          vec2 q = mat2(cos(an),-sin(an),sin(an),cos(an))*(f - (vec2(hash(id+1.3), hash(id+2.7))-0.5)*0.5);
          q.x /= 0.15 + 0.85*abs(sin(iTime*3.0 + h*9.0));
          float sq = step(max(abs(q.x), abs(q.y)), 0.13);
          col = mix(col, hsv(h, 0.75, 1.0), sq*step(0.4, h));
        }
      } else if (m == 14) {
        // dotmatrix: a panel of round LEDs, one column per band
        vec2 g = vec2(32.0, 18.0);
        vec2 cell = floor(sc*g);
        vec2 f = fract(sc*g)-0.5;
        float v = specLog((cell.x+0.5)/g.x);
        float on = step(cell.y+0.5, v*g.y*1.1);
        vec3 led = hsv(0.33 - 0.33*cell.y/g.y, 0.9, 1.0);
        col = led*smoothstep(0.42, 0.3, length(f))*(on + 0.07);
      } else if (m == 15) {
        // mirrorbars: the analyser mirrored round the middle, with a reflection
        float n = 48.0;
        float bi = floor(sc.x*n);
        float fx = fract(sc.x*n);
        float v = specLog(abs((bi+0.5)/n - 0.5)*2.0);
        float y = sc.y - 0.42;
        float h = v*0.5;
        float bar = step(0.12, fx)*step(fx, 0.88);
        vec3 c = pal(v + (bi/n)*0.3 + t*0.1, vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0,0.33,0.67));
        if (y >= 0.0) col = c*bar*step(y, h)*(0.5 + 0.6*y/(h+0.001));
        else col = c*bar*step(-y, h*0.5)*0.35*(1.0 + y/(h*0.5+0.001));
        col += vec3(0.6)*smoothstep(0.004,0.0,abs(y))*0.4;
      } else if (m == 16) {
        // blobring: a ring pushed out by the spectrum all the way round
        float f = abs(a)/3.14159;
        float rad = 0.2 + specLog(f)*0.2 + uBass*0.04;
        col = hsv(fract(f*0.5 + t*0.2), 0.7, 1.0)*(0.004/(abs(r - rad)+0.004));
        col += hsv(fract(f*0.5 + t*0.2 + 0.5), 0.6, 1.0)*0.3*smoothstep(rad, 0.0, r)*uLevel;
      } else if (m == 17) {
        // interference: three wave sources drifting, their ripples crossing
        float w = 0.0;
        for (int k=0;k<3;k++){
          float fk = float(k);
          vec2 src = 0.35*vec2(cos(t*(0.7+fk*0.3)+fk*2.1), sin(t*(0.5+fk*0.4)+fk*1.3));
          w += sin(length(uv-src)*(40.0+uMid*30.0) - iTime*(3.0+uBass*4.0));
        }
        w /= 3.0;
        col = pal(w*0.5+0.5 + t*0.05, vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0,0.1,0.2))*(0.25 + 0.75*w*w);
      } else if (m == 18) {
        // zoomsquares: squares nested forever, falling inward, each turned a bit more
        float z = fract(iTime*(0.15 + uBass*0.25));
        for (int k=0;k<9;k++){
          float fk = float(k);
          float s = pow(1.6, fk - z)*0.03;
          float ro = (fk - z)*0.35 + t*0.2;
          vec2 q = mat2(cos(ro),-sin(ro),sin(ro),cos(ro))*uv;
          float d = abs(max(abs(q.x),abs(q.y)) - s);
          col += hsv(fract(fk*0.12 - z*0.12 + t*0.1), 0.7, 1.0)
               * smoothstep(0.004 + s*0.015, 0.0, d)*(0.6 + spec(fk/9.0)*1.5);
        }
      } else if (m == 19) {
        // argyle: a diamond lattice with stitched lines, colours stepping along
        float sz = 4.0 + uMid;
        vec2 p = uv*sz;
        p = vec2(p.x+p.y, p.x-p.y)*0.7071 + vec2(t*0.5, 0.0);
        vec2 id = floor(p);
        float k = mod(id.x+id.y, 2.0);
        float step1 = floor(iTime*0.5);
        vec3 c1 = hsv(fract(0.55 + step1*0.1), 0.6, 0.8);
        vec3 c2 = hsv(fract(0.05 + step1*0.1), 0.7, 0.6);
        col = mix(c1, c2, k)*(0.55 + 0.6*spec(fract(id.x*0.13 + id.y*0.07)));
        vec2 q = uv*sz*2.0;
        float st = min(abs(fract(q.x+q.y+t)-0.5), abs(fract(q.x-q.y)-0.5));
        col += vec3(1.0)*smoothstep(0.03, 0.0, st)*0.35*(0.5 + uTreble);
      } else if (m == 20) {
        // rose: petal curves r = cos(k*theta), the petal count set by the mids
        float kk = 2.0 + floor(uMid*6.0);
        float glow = 0.0;
        for (int i=0;i<3;i++){
          float fi = float(i);
          float rr = (0.35 + fi*0.03)*abs(cos(kk*(a + t*(0.3+fi*0.2))))*(0.8 + uBass*0.4);
          glow += 0.004/(abs(r - rr)+0.004)*(1.0 - fi*0.25);
        }
        col = hsv(fract(t*0.2 + r), 0.6, 1.0)*glow;
      } else if (m == 21) {
        // sonar: a radar sweep, blips where the spectrum peaks
        float sweep = mod(iTime*1.2, 6.2831);
        float inside = step(r, 0.45);
        col = vec3(0.1,1.0,0.4)*exp(-mod(sweep - (a + 3.14159), 6.2831)*3.0)*0.6*inside;
        col += vec3(0.1,0.6,0.3)*smoothstep(0.0025, 0.0, abs(fract(r*10.0+0.5)-0.5)*0.1)*inside*0.5;
        col += vec3(0.1,0.6,0.3)*smoothstep(0.002, 0.0, min(abs(uv.x), abs(uv.y)))*inside*0.4;
        float bin = floor((a + 3.14159)/6.2831*32.0);
        float ba = (bin + 0.5)/32.0*6.2831;
        float bv = spec((bin + 0.5)/32.0);
        vec2 bp = (0.08 + 0.34*bv)*vec2(cos(ba - 3.14159), sin(ba - 3.14159));
        col += vec3(0.5,1.0,0.6)*smoothstep(0.02, 0.0, length(uv-bp))*exp(-mod(sweep - ba, 6.2831)*1.2)*step(0.15, bv);
        col += vec3(0.1,0.5,0.25)*smoothstep(0.004, 0.0, abs(r-0.452));
      } else if (m == 22) {
        // heartbeat: an ECG trace sweeping across, spiking with the bass
        col = vec3(0.0,0.03,0.02);
        vec2 gg = abs(fract(sc*vec2(20.0,12.0))-0.5);
        col += vec3(0.0,0.12,0.06)*smoothstep(0.46, 0.5, max(gg.x, gg.y));
        float head = fract(iTime*0.25);
        float age = fract(head - sc.x);
        float amp = 0.08 + specAt(0.02, min(age*2.0, 1.0))*0.45;
        float e = 1.0/iResolution.x;
        float y0 = 0.42 + ecgShape(sc.x)*amp;
        float y1 = 0.42 + ecgShape(sc.x + e)*amp;
        float slope = (y1 - y0)*iResolution.y;
        float dpx = abs(sc.y - y0)*iResolution.y/sqrt(1.0 + slope*slope);
        col += vec3(0.2,1.0,0.5)*smoothstep(2.0, 0.5, dpx)*exp(-age*3.0);
        float amp0 = 0.08 + specAt(0.02, 0.0)*0.45;
        col += vec3(0.8,1.0,0.8)*smoothstep(0.012, 0.0, length((sc - vec2(head, 0.42 + ecgShape(head)*amp0))*vec2(asp, 1.0)));
      } else if (m == 23) {
        // trimeter: three segmented meters for the bass, the mids and the treble
        float mc = floor(sc.x*3.0);
        float fx = fract(sc.x*3.0);
        float v = mc < 0.5 ? uBass : (mc < 1.5 ? uMid : uTreble);
        v = clamp(v*1.4, 0.0, 1.0);
        float seg = floor(sc.y*20.0);
        float fy = fract(sc.y*20.0);
        float on = step(seg/20.0, v - 0.001);
        float box = step(0.15,fx)*step(fx,0.85)*step(0.15,fy)*step(fy,0.85);
        col = hsv(0.33 - 0.33*seg/19.0, 0.85, 1.0)*box*(on + 0.08);
      } else if (m == 24) {
        // plasmaglobe: tendrils reaching from the core to the glass
        float glow = 0.0;
        for (int k=0;k<6;k++){
          float fk = float(k);
          float ta = fk*1.047 + sin(t*1.3 + fk*2.0)*0.6
                   + (fbm(vec2(r*4.0 - iTime*0.8, fk)) - 0.5)*2.4*(0.6 + uTreble);
          float da = abs(mod(a - ta + 3.14159, 6.2831) - 3.14159);
          glow += 0.025*(0.6 + uBass)/(da*r*5.0 + 0.025)*smoothstep(0.46, 0.1, r);
        }
        col = vec3(0.7,0.35,1.0)*glow*0.35 + vec3(1.0,0.85,1.0)*pow(glow*0.25, 2.0);
        col += vec3(1.0,0.7,1.0)*smoothstep(0.07, 0.0, r)*(0.8 + uBass);
        col += vec3(0.4,0.3,0.7)*smoothstep(0.004, 0.0, abs(r - 0.46))*0.8;
      } else if (m == 25) {
        // galaxy: two spiral arms of stars and dust round a bright core
        float rr = r*(1.4 - uBass*0.2);
        float arm = sin(2.0*a - log(rr+0.02)*5.0 + iTime*0.3);
        float dust = fbm(vec2(a*2.0 + log(rr+0.02)*3.0 - iTime*0.15, rr*6.0));
        float band = smoothstep(0.1, 1.0, arm*0.5+0.5)*exp(-rr*3.0);
        col = mix(vec3(0.3,0.4,1.0), vec3(1.0,0.6,0.8), dust)*band*(0.8 + uMid*0.8);
        col += vec3(1.0,0.9,0.7)*exp(-rr*18.0)*(0.8 + uBass);
        vec2 sp = floor(gl_FragCoord.xy*0.7);
        col += step(0.985, hash(sp))*(0.3 + hash(sp+1.0))*0.7*(0.6 + uTreble*0.8);
      } else if (m == 26) {
        // caustics: light rippling across a pool floor
        vec2 p = uv*3.0 + vec2(iTime*0.1, 0.0);
        vec2 w = vec2(fbm(p + iTime*0.2), fbm(p + 5.2 - iTime*0.15));
        float n = fbm(p*1.5 + w*2.0*(0.8 + uBass*0.6));
        float lines = pow(1.0 - abs(n*2.0 - 1.0), 8.0);
        col = vec3(0.0,0.22,0.32) + vec3(0.6,0.95,1.0)*lines*(0.8 + uLevel);
      } else if (m == 27) {
        // lavalamp: slow wax blobs rising and sinking, merging, warmed by the bass
        float f = 0.0;
        for (int k=0;k<6;k++){
          float fk = float(k);
          vec2 c = vec2(sin(fk*2.3 + t*0.7)*0.2*asp, sin(t*(0.6 + hash(vec2(fk,1.0))*0.8) + fk*1.7)*0.4);
          float rad = 0.08 + 0.05*hash(vec2(fk,2.0)) + uBass*0.03;
          f += rad*rad/max(dot(uv-c, uv-c), 0.0001);
        }
        float blob = smoothstep(0.9, 1.1, f);
        vec3 wax = mix(vec3(1.0,0.25,0.1), vec3(1.0,0.75,0.2), clamp(f-1.0, 0.0, 1.0));
        vec3 bg = mix(vec3(0.25,0.0,0.3), vec3(0.6,0.1,0.35), sc.y);
        col = mix(bg, wax, blob) + vec3(1.0,0.5,0.3)*smoothstep(0.6, 1.0, f)*(1.0 - blob)*0.3;
      } else if (m == 28) {
        // tileflip: a wall of tiles, each column turning over to its band's colour
        vec2 g = vec2(16.0, 9.0);
        vec2 id = floor(sc*g);
        vec2 f = fract(sc*g);
        float v = specLog((id.x+0.5)/g.x);
        float ph = clamp(v*1.4 - id.y/g.y + 0.2, 0.0, 1.0);
        float w = abs(cos(ph*3.14159));
        float inside = step(abs(f.x-0.5), 0.45*w)*step(abs(f.y-0.5), 0.45);
        vec3 back = hsv(fract(id.x/g.x*0.8 + t*0.1), 0.75, 1.0);
        col = mix(vec3(0.12,0.12,0.16), back, step(0.5, ph))*inside*(0.7 + 0.3*w);
      } else if (m == 29) {
        // zebra: stripes pulled through a flowing warp
        vec2 p = uv*2.0;
        vec2 w = vec2(fbm(p + t), fbm(p - t + 3.3));
        float s = sin((p.x + w.x*(2.0 + uBass*2.0))*12.0 + (p.y + w.y)*4.0);
        col = mix(vec3(0.03), mix(vec3(1.0), hsv(fract(t*0.2), 0.6, 1.0), uMid), smoothstep(-0.15, 0.15, s));
      } else if (m == 30) {
        // chroma: rings split into red, green and blue by the bass
        float sp = 0.004 + uBass*0.03;
        float k = 18.0 + uMid*10.0;
        float wob = 0.02*sin(a*6.0 + t*3.0)*uTreble;
        float r1 = length(uv - vec2(sp, 0.0)) + wob, r3 = length(uv + vec2(sp, 0.0)) + wob;
        col = vec3(pow(0.5+0.5*sin(r1*k*6.2831 - iTime*2.0), 6.0),
                   pow(0.5+0.5*sin((r+wob)*k*6.2831 - iTime*2.0), 6.0),
                   pow(0.5+0.5*sin(r3*k*6.2831 - iTime*2.0), 6.0))*1.2;
      } else if (m == 31) {
        // blackhole: a glowing accretion disk bent round a dark centre
        vec2 dp = vec2(uv.x, uv.y/0.35);
        float dr = length(dp);
        float da = atan(dp.y, dp.x);
        float disk = smoothstep(0.12, 0.16, dr)*smoothstep(0.55, 0.25, dr);
        float swirl = fbm(vec2(da*3.0 - iTime*1.2/(dr+0.1), dr*8.0));
        vec3 dc = mix(vec3(1.0,0.35,0.05), vec3(1.0,0.9,0.6), swirl)*disk*(0.6 + swirl)*(0.8 + uLevel);
        float hole = smoothstep(0.1, 0.095, r);
        float arc = exp(-abs(r - 0.15 - uBass*0.02)*40.0)*smoothstep(-0.05, 0.1, uv.y);
        vec3 back = (dc*step(0.0, uv.y) + vec3(1.0,0.6,0.25)*arc*(0.7 + uMid))*(1.0 - hole);
        col = back + vec3(1.0,0.85,0.6)*exp(-abs(r - 0.102)*120.0)*0.8 + dc*step(uv.y, 0.0);
      } else if (m == 32) {
        // sunrays: a pulsing sun behind drifting cloud, rays fanning out
        vec2 d = uv - vec2(0.0, 0.15);
        float ang = atan(d.y, d.x);
        float rays = pow(0.5+0.5*sin(ang*14.0 + iTime*0.3), 3.0)*0.6 + pow(0.5+0.5*sin(ang*23.0 - iTime*0.2), 4.0)*0.4;
        float cloud = smoothstep(0.4, 0.75, fbm(uv*2.5 + vec2(iTime*0.05, 0.0)));
        col = vec3(1.0,0.8,0.45)*rays*exp(-length(d)*2.2)*(0.5 + uBass*1.2)*(1.0 - cloud*0.6);
        col += vec3(1.0,0.95,0.8)*smoothstep(0.09 + uBass*0.02, 0.07, length(d));
        col = mix(col, vec3(0.9,0.8,0.7)*(0.3 + uMid*0.4), cloud*0.5);
        col += vec3(0.25,0.3,0.55)*smoothstep(0.5, -0.5, uv.y)*0.3;
      } else if (m == 33) {
        // cybergrid: flying down a corridor between two neon grids
        float z = 0.1/(abs(uv.y) + 0.001);
        vec2 g = vec2(uv.x*z + 0.5, z + iTime*(1.5 + uBass*3.0));
        vec2 f = abs(fract(g)-0.5);
        float lw = clamp(0.03*z, 0.02, 0.4);
        float line = smoothstep(0.5 - lw, 0.5, max(f.x, f.y));
        vec3 c = uv.y > 0.0 ? vec3(0.2,0.6,1.0) : vec3(1.0,0.2,0.8);
        col = c*line*exp(-z*0.07)*(0.7 + uMid);
        col += c*exp(-abs(uv.y)*30.0)*0.5*(0.6 + uLevel);
        float eq = specLog(clamp(abs(uv.x)/(0.5*asp), 0.0, 1.0));
        col += vec3(1.0,0.9,1.0)*smoothstep(0.004, 0.0, abs(abs(uv.y) - eq*0.07))*0.8;
      } else if (m == 34) {
        // halftone: printed dots swelling with a travelling wave
        float n = 26.0;
        vec2 p = mat2(0.966,-0.259,0.259,0.966)*uv*n;
        vec2 id = floor(p) + 0.5;
        vec2 f = fract(p) - 0.5;
        float cr = length(id)/n;
        float wave = 0.5 + 0.5*sin(cr*10.0 - iTime*2.0*(1.0 + uBass));
        float v = clamp(wave*0.7 + spec(fract(cr*1.2))*0.5, 0.0, 1.0);
        col = hsv(fract(cr*0.5 + t*0.1), 0.7, 1.0)*smoothstep(v*0.5 + 0.03, v*0.5 - 0.03, length(f));
      } else if (m == 35) {
        // spirograph: a hypotrochoid being drawn, the gear ratio drifting
        float R = 0.3, rr0 = 0.11 + uMid*0.04, dd = 0.12 + uBass*0.06;
        float q = (R - rr0)/rr0;
        float glow = 0.0;
        vec2 prev = vec2(0.0);
        for (int k=0;k<40;k++){
          float s = iTime*1.5 - float(k)*0.05;
          vec2 pt = vec2((R-rr0)*cos(s) + dd*cos(q*s), (R-rr0)*sin(s) - dd*sin(q*s));
          if (k > 0) glow += 0.002/(sdSeg(uv, prev, pt) + 0.002)*(1.0 - float(k)/40.0);
          prev = pt;
        }
        col = hsv(fract(t*0.3), 0.6, 1.0)*glow;
      } else if (m == 36) {
        // starpoly: nested star polygons turning against each other
        float glow = 0.0;
        for (int k=0;k<5;k++){
          float fk = float(k);
          float n = 5.0 + fk;
          float aa = mod(a + t*(0.5 - fk*0.2), 6.2831/n) - 3.14159/n;
          vec2 q = r*vec2(cos(aa), abs(sin(aa)));
          float ro = 0.1 + fk*0.07 + spec(fk/5.0)*0.04;
          vec2 B = ro*0.45*vec2(cos(3.14159/n), sin(3.14159/n));
          glow += 0.003/(sdSeg(q, vec2(ro, 0.0), B) + 0.003)*(1.0 - fk*0.12);
        }
        col = hsv(fract(t*0.15 + r), 0.7, 1.0)*glow*0.8;
      } else if (m == 37) {
        // pixelrain: chunky 8-bit blocks falling, one column per band
        vec2 g = vec2(24.0, 18.0);
        float cx = floor(sc.x*g.x);
        float v = specLog((cx+0.5)/g.x);
        float y = sc.y*g.y + iTime*(1.0 + v*6.0 + hash(vec2(cx,1.0))*2.0);
        float on = step(0.72 - v*0.4, hash(vec2(cx, floor(y))));
        vec2 f = fract(vec2(sc.x*g.x, y));
        float box = step(0.08,f.x)*step(f.x,0.92)*step(0.08,f.y)*step(f.y,0.92);
        col = hsv(fract(cx/g.x*0.7 + 0.55), 0.75, 1.0)*on*box*(0.35 + v);
      } else if (m == 38) {
        // circlegram: the spectrogram wrapped round a ring, newest on the outside
        float inner = 0.12, outer = 0.46;
        float age = clamp((outer - r)/(outer - inner), 0.0, 1.0);
        float v = specAt(pow(abs(a)/3.14159, 1.6), age);
        col = pal(v*0.9 + 0.1, vec3(0.5), vec3(0.5), vec3(1.0), vec3(0.0,0.1,0.2))*v*step(inner, r)*step(r, outer)*1.3;
        col += vec3(0.8)*smoothstep(0.004, 0.0, abs(r - outer))*0.3;
      } else if (m == 39) {
        // eye: an iris whose pupil opens with the bass, glancing about
        vec2 p = uv - 0.05*vec2(sin(t*1.3), cos(t*0.9)*0.6);
        float pr = length(p);
        float pa = atan(p.y, p.x);
        float pupil = 0.06 + uBass*0.06;
        float lid = abs(uv.y) - 0.28*(1.0 - pow(abs(uv.x)/0.55, 2.0));
        float blink = step(0.97, fract(iTime*0.12));
        float open = smoothstep(0.01, -0.01, lid + blink*0.3);
        vec3 e = vec3(0.92,0.9,0.88)*(1.0 - 0.5*pow(pr/0.5, 2.0));
        float fibres = 0.5 + 0.5*sin(pa*40.0 + fbm(vec2(pa*3.0, pr*10.0))*6.0);
        vec3 ic = mix(vec3(0.1,0.35,0.3), vec3(0.4,0.8,0.6), fibres*smoothstep(pupil, 0.2, pr));
        ic = mix(ic, hsv(fract(t*0.1), 0.6, 0.8), uMid*0.5);
        e = mix(e, ic, smoothstep(0.205, 0.195, pr));
        e = mix(e, vec3(0.0), smoothstep(pupil + 0.005, pupil - 0.005, pr));
        e += vec3(1.0)*smoothstep(0.03, 0.0, length(p - vec2(-0.06, 0.06)))*0.8;
        col = e*open;
      } else if (m == 40) {
        // tessellate: a triangle mosaic, tiles lighting up in turn
        vec2 p = uv*(5.0 + uMid*2.0) + vec2(t*0.6, 0.0);
        p.y *= 1.1547;
        p.x += p.y*0.5;
        vec2 id = floor(p);
        vec2 f = fract(p);
        float up = step(f.y, f.x);
        float h = hash(id*2.0 + vec2(up, 0.0));
        float pulse = pow(0.5 + 0.5*sin(iTime*2.0 + h*6.2831 + length(id)*0.5), 4.0);
        vec3 c = pal(h + t*0.05, vec3(0.5), vec3(0.4), vec3(1.0), vec3(0.0,0.33,0.67));
        float edge = min(min(min(f.x, 1.0-f.x), min(f.y, 1.0-f.y)), abs(f.x - f.y)*0.707);
        col = c*(0.25 + pulse*(0.6 + uBass))*smoothstep(0.0, 0.04, edge);
      } else if (m == 41) {
        // flower: petals whose length is the spectrum, turning slowly
        float petals = 12.0;
        float u2 = fract((a + t*0.3)/6.2831)*petals;
        float seg = floor(u2);
        float local = fract(u2) - 0.5;
        float len = 0.12 + specLog((seg + 0.5)/petals)*0.33;
        float edge = len*sqrt(max(1.0 - abs(local)*2.0, 0.0));
        float petal = smoothstep(edge + 0.01, edge - 0.01, r)*step(0.03, r);
        col = hsv(fract(seg/petals*0.3 + t*0.1), 0.7, 1.0)*petal*(0.4 + r/len*0.8);
        col += vec3(1.0,0.9,0.4)*smoothstep(0.05, 0.03, r)*(0.8 + uBass);
      } else if (m == 42) {
        // orbits: planets on tilted rings round a sun, leaving trails
        col = vec3(1.0,0.8,0.4)*0.03/(r + 0.02)*(0.6 + uBass);
        vec2 e = uv/vec2(1.0, 0.45);
        for (int k=0;k<5;k++){
          float fk = float(k);
          float R = 0.1 + fk*0.075;
          float ang = iTime*1.2/(fk + 1.0) + fk*2.0;
          vec2 pp = R*vec2(cos(ang), sin(ang)*0.45);
          float pr2 = 0.016 + 0.014*spec(fk/5.0) + fk*0.003;
          col += hsv(fract(fk*0.19 + 0.05), 0.6, 1.0)*smoothstep(pr2, pr2*0.5, length(uv - pp))*1.2;
          col += vec3(0.25)*smoothstep(0.003, 0.0, abs(length(e) - R))*0.25;
        }
      } else if (m == 43) {
        // wirecube: a spinning wireframe cube, demo-scene style, kicked by the bass
        float ay = iTime*0.6, ax = iTime*0.37;
        mat3 ry = mat3(cos(ay),0.0,sin(ay), 0.0,1.0,0.0, -sin(ay),0.0,cos(ay));
        mat3 rx = mat3(1.0,0.0,0.0, 0.0,cos(ax),-sin(ax), 0.0,sin(ax),cos(ax));
        float s = 0.22*(1.0 + uBass*0.35);
        float glow = 0.0;
        for (int ai=0; ai<3; ai++){
          for (int ei=0; ei<4; ei++){
            float fe = float(ei);
            vec2 sg = vec2(mod(fe, 2.0)*2.0 - 1.0, floor(fe/2.0)*2.0 - 1.0);
            vec3 p0 = vec3(-1.0, sg);
            vec3 p1 = vec3(1.0, sg);
            if (ai == 1) { p0 = vec3(sg.x, -1.0, sg.y); p1 = vec3(sg.x, 1.0, sg.y); }
            if (ai == 2) { p0 = vec3(sg, -1.0); p1 = vec3(sg, 1.0); }
            p0 = rx*(ry*p0)*s;
            p1 = rx*(ry*p1)*s;
            vec2 A = p0.xy*1.2/(1.2 + p0.z);
            vec2 B = p1.xy*1.2/(1.2 + p1.z);
            glow += 0.0025/(sdSeg(uv, A, B) + 0.002);
          }
        }
        col = hsv(fract(t*0.1 + 0.45), 0.6, 1.0)*glow*0.6;
      } else if (m == 44) {
        // orb: a glossy blob bulging with the bass (ray marched)
        vec3 ro = vec3(0.0, 0.0, -2.4);
        vec3 rd = normalize(vec3(uv, 1.3));
        float tt = 0.0;
        float hit = 0.0;
        vec3 pos = ro;
        for (int k=0;k<48;k++){
          pos = ro + rd*tt;
          float d = orbSDF(pos);
          if (d < 0.002) { hit = 1.0; break; }
          tt += d*0.6;
          if (tt > 5.0) break;
        }
        col = vec3(0.02,0.02,0.05) + vec3(0.1,0.05,0.2)*(1.0 - r);
        if (hit > 0.5) {
          vec2 e = vec2(0.003, -0.003);
          vec3 n = normalize(e.xyy*orbSDF(pos + e.xyy) + e.yyx*orbSDF(pos + e.yyx)
                           + e.yxy*orbSDF(pos + e.yxy) + e.xxx*orbSDF(pos + e.xxx));
          vec3 l = normalize(vec3(0.6, 0.7, -0.8));
          float dif = max(dot(n, l), 0.0);
          float fres = pow(1.0 - max(dot(n, -rd), 0.0), 3.0);
          col = hsv(fract(t*0.15 + n.y*0.2), 0.6, 0.9)*(0.15 + dif*0.8)
              + vec3(1.0)*pow(max(dot(reflect(rd, n), l), 0.0), 30.0)
              + fres*hsv(fract(t*0.15 + 0.5), 0.5, 1.0)*(0.5 + uTreble);
        }
      } else if (m == 45) {
        // pendulum: a row of pendulums of slightly different lengths, drifting
        // in and out of step
        for (int k=0;k<15;k++){
          float fk = float(k);
          vec2 pb = vec2((fk - 7.0)/7.0*0.4*asp*0.9, sin(iTime*(1.6 + fk*0.07))*0.3);
          float rad = 0.025 + spec(fk/15.0)*0.025;
          col += hsv(fk/15.0, 0.65, 1.0)*smoothstep(rad, rad*0.3, length(uv - pb))*1.3;
        }
      } else if (m == 46) {
        // dunes: desert ridges at dusk, the heat shimmer set by the treble
        col = mix(vec3(1.0,0.6,0.3), vec3(0.3,0.2,0.5), smoothstep(-0.1, 0.5, uv.y));
        col += vec3(1.0,0.8,0.5)*exp(-length(uv - vec2(-0.3*asp, 0.02))*5.0)*(0.6 + uBass*0.6);
        vec2 q = uv + vec2(0.0, sin(uv.x*40.0 + iTime*6.0)*0.006*uTreble);
        for (int k=0;k<4;k++){
          float fk = float(k);
          float ph = q.x*(2.0 + fk) + fk*1.7 + t*(0.2 + fk*0.1);
          float h = -0.05 - fk*0.12 + 0.06*sin(ph) + 0.03*sin(q.x*(5.0 + fk*2.0) - fk)
                  + spec(fract(sc.x*0.5 + fk*0.2))*0.02;
          vec3 sand = mix(vec3(0.45,0.22,0.12), vec3(0.95,0.6,0.35), 0.5 + 0.5*cos(ph));
          sand = mix(sand, col, (1.0 - fk/3.0)*0.35);
          if (q.y < h) col = sand;
        }
      } else if (m == 47) {
        // prism: a white beam splitting into a rainbow fan, spread by the bass
        vec2 q = uv - vec2(0.0, -0.02);
        float tri = max(abs(q.x)*0.866 + q.y*0.5, -q.y) - 0.1;
        col = vec3(0.1,0.12,0.18)*step(tri, 0.0) + vec3(0.9,0.95,1.0)*smoothstep(0.004, 0.0, abs(tri))*0.8;
        col += vec3(1.0)*0.004/(sdSeg(uv, vec2(-0.6*asp, -0.2), vec2(-0.07, -0.01)) + 0.004)*(0.5 + uLevel);
        if (uv.x > 0.06) {
          vec2 d = uv - vec2(0.06, -0.01);
          float spread = 0.12 + uBass*0.25;
          float u = (atan(d.y, d.x) + 0.25 + spread)/(2.0*spread);
          col += hsv(u*0.8, 0.9, 1.0)*step(0.0, u)*step(u, 1.0)*(0.5 + spec(clamp(u,0.0,1.0))*1.5)*smoothstep(0.0, 0.1, length(d))*0.7;
        }
      } else if (m == 48) {
        // tenprint: the C64 one-liner maze scrolling by, loading-screen border on the bass
        vec2 p = uv*(10.0 + uMid*4.0) + vec2(0.0, iTime*0.8);
        vec2 id = floor(p);
        vec2 f = fract(p);
        float d = hash(id) > 0.5 ? abs(f.x - f.y) : abs(f.x + f.y - 1.0);
        vec3 bg = vec3(0.21,0.16,0.62);
        col = mix(bg, vec3(0.49,0.44,0.95), smoothstep(0.12, 0.05, d*0.707))*(0.6 + uLevel*0.6);
        if (abs(uv.x) > 0.44*asp || abs(uv.y) > 0.44) {
          col = mix(vec3(0.49,0.44,0.95), hsv(fract(floor(sc.y*40.0 + iTime*25.0)*0.37), 0.8, 0.9), step(0.3, uBass));
        }
      } else {
        // crt: an old TV finding its picture, static giving way to colour bars
        vec2 cc = sc - 0.5;
        vec2 p = 0.5 + cc*(1.0 + dot(cc, cc)*0.25);
        vec3 bars = hsv(floor(p.x*7.0)/7.0, 0.8, 0.9)*(0.4 + spec(floor(p.x*7.0)/7.0 + 0.07)*1.2);
        float stat = hash(floor(p*vec2(160.0, 120.0)) + floor(iTime*30.0));
        col = mix(bars, vec3(stat), clamp(0.85 - uLevel*1.5, 0.1, 0.9));
        col *= 0.75 + 0.25*sin(p.y*iResolution.y*1.5);
        col *= 1.0 - 0.35*exp(-pow((fract(p.y + iTime*0.15*(0.5 + uBass)) - 0.5)*8.0, 2.0));
        if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0) col = vec3(0.0);
      }

      // Scenes and instrument displays light themselves (see the first bank).
      bool selfLit = m==0 || m==1 || m==2 || m==8 || m==9 || m==10 || m==14 || m==15
                  || m==21 || m==22 || m==23 || m==27 || m==28 || m==33 || m==38
                  || m==39 || m==46 || m==48 || m==49;
      if (!selfLit) {
        col *= 0.35+0.9*uLevel+0.3*uBass;
        col *= smoothstep(1.5, 0.15, r);
      }

      float fb = feedbackForB(m);
      if (fb > 0.0) {
        float fa = 0.03 + 0.05*sin(t*0.6 + float(m + 50)) + uMid*0.08;
        float fz = 0.988 - uBass*0.020;
        mat2 frot = mat2(cos(fa), -sin(fa), sin(fa), cos(fa));
        vec2 fq = frot * uv * fz;
        vec3 prev = texture2D(uPrev, vec2(fq.x*iResolution.y/iResolution.x, fq.y) + 0.5).rgb;
        prev = prev*(fb - 0.02*uTreble) - 0.006;
        prev *= smoothstep(1.6, 0.2, r);
        col = max(col, max(prev, vec3(0.0)));
      }
      gl_FragColor = vec4(clamp(col,0.0,1.0),1.0);
    }
  `;
