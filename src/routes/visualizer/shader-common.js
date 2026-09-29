// Shared by both visualizer shader programs (modes 1-50 and 51-100): the
// uniforms the page sets every frame and the helper functions the patterns use.
export const SHADER_COMMON = `
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
`;
