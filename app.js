// Spotiamp+ website: the spectrum behind the hero, the skins going round in
// the hero's screen, the skin picker, the latest version, and sections that
// fade in as you reach them. Nothing here phones home but the version check
// (GitHub's public API).
(() => {
  const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const SKINS = ["spotiamp", "classic", "bento", "winamp3", "winamp5", "nucleo", "sonycdx", "esprit", "cherry", "amber", "emerald"];
  const NAMES = {
    spotiamp: "SPOTIAMP+", classic: "WINAMP CLASSIC", bento: "BENTO CLASSIFIED", winamp3: "WINAMP3 CLASSIFIED",
    winamp5: "WINAMP5 CLASSIFIED", nucleo: "NUCLEO NLOG", sonycdx: "SONY CDX-MP3", esprit: "SONY ESPRIT V2",
    cherry: "CHERRY", amber: "AMBER", emerald: "EMERALD",
  };
  const src = (skin) => `assets/skin-${skin}.webp`;

  // --- the spectrum: Winamp's bars and falling peaks, in the logo's colours
  const canvas = document.querySelector(".spectrum");
  if (canvas && !reduced) {
    const ctx = canvas.getContext("2d");
    const BARS = 64;
    const level = new Float32Array(BARS);
    const peak = new Float32Array(BARS);
    let w = 0, h = 0, dpr = 1;
    const size = () => {
      dpr = Math.min(2, devicePixelRatio || 1);
      w = canvas.clientWidth; h = canvas.clientHeight;
      canvas.width = w * dpr; canvas.height = h * dpr;
    };
    size();
    addEventListener("resize", size);
    let t = 0;
    const frame = () => {
      t += 1 / 60;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const bw = w / BARS;
      const beat = Math.pow(Math.max(0, Math.sin(t * Math.PI * 2 * 2.05)), 8);
      for (let i = 0; i < BARS; i++) {
        const x = i / BARS;
        // a falling-off spectrum that moves like music: bass, mids, a beat
        const shape = 0.85 * Math.exp(-x * 2.2) + 0.25;
        const wobble = 0.5 + 0.5 * Math.sin(t * (1.3 + x * 3.1) + i * 0.7) * Math.sin(t * 0.7 + i * 1.3);
        const target = Math.min(1, shape * (0.35 + 0.65 * wobble) + beat * 0.35 * Math.exp(-x * 4));
        level[i] += (target - level[i]) * (target > level[i] ? 0.35 : 0.08);
        peak[i] = Math.max(peak[i] - 0.006, level[i]);
        const bh = level[i] * h * 0.8;
        const g = ctx.createLinearGradient(0, h, 0, h - bh);
        g.addColorStop(0, "rgba(255, 90, 10, 0.06)");
        g.addColorStop(0.5, "rgba(255, 110, 20, 0.30)");
        g.addColorStop(1, "rgba(255, 190, 40, 0.55)");
        ctx.fillStyle = g;
        ctx.fillRect(i * bw + 2, h - bh, bw - 4, bh);
        ctx.fillStyle = "rgba(255, 210, 90, 0.8)";
        ctx.fillRect(i * bw + 2, h - peak[i] * h * 0.8 - 3, bw - 4, 2);
      }
      requestAnimationFrame(frame);
    };
    requestAnimationFrame(frame);
  }

  // --- the hero's screen: the skins, one after another
  const screen = document.querySelector(".screen");
  const lcd = document.querySelector(".lcd");
  if (screen) {
    const first = screen.querySelector(".shot");
    const shots = [first];
    // the others load once the page has, so the first paint stays quick
    addEventListener("load", () => {
      for (const skin of SKINS.slice(1)) {
        const img = new Image();
        img.className = "shot";
        img.alt = "";
        img.src = src(skin);
        img.width = 1090; img.height = 748;
        img.dataset.skin = skin;
        screen.insertBefore(img, screen.querySelector("figcaption"));
        shots.push(img);
      }
      first.dataset.skin = "spotiamp";
      if (reduced) return;
      let at = 0;
      setInterval(() => {
        if (document.hidden) return;
        shots[at].classList.remove("on");
        at = (at + 1) % shots.length;
        shots[at].classList.add("on");
        if (lcd) lcd.textContent = NAMES[shots[at].dataset.skin] ?? "SPOTIAMP+";
      }, 2600);
    });
  }

  // --- the skin picker
  const view = document.querySelector(".skin-view img");
  for (const chip of document.querySelectorAll(".chip")) {
    chip.addEventListener("click", () => {
      for (const c of document.querySelectorAll(".chip")) {
        c.classList.toggle("on", c === chip);
        c.setAttribute("aria-selected", String(c === chip));
      }
      const skin = chip.dataset.skin;
      view.classList.add("fading");
      const next = new Image();
      next.onload = () => {
        view.src = next.src;
        view.alt = `Spotiamp+ in the ${chip.textContent} skin`;
        view.classList.remove("fading");
      };
      next.src = src(skin);
    });
  }

  // --- the latest version, from GitHub (the page says v0.7.8 until it knows)
  fetch("https://api.github.com/repos/fdeox/spotiamp-plus/releases/latest")
    .then((r) => (r.ok ? r.json() : null))
    .then((rel) => {
      if (rel?.tag_name) for (const v of document.querySelectorAll(".version")) v.textContent = rel.tag_name;
    })
    .catch(() => {});

  // --- the stars on GitHub, next to the version
  fetch("https://api.github.com/repos/fdeox/spotiamp-plus")
    .then((r) => (r.ok ? r.json() : null))
    .then((repo) => {
      const stars = repo?.stargazers_count;
      const link = document.querySelector(".stars");
      if (!link || !stars) return;
      link.querySelector(".star-count").textContent = stars;
      link.hidden = false;
    })
    .catch(() => {});

  // --- sections fade in as you scroll to them
  if (!reduced && "IntersectionObserver" in window) {
    const io = new IntersectionObserver((entries) => {
      for (const e of entries) if (e.isIntersecting) { e.target.classList.add("seen"); io.unobserve(e.target); }
    }, { rootMargin: "0px 0px -8% 0px" });
    for (const el of document.querySelectorAll(".section .head, .card, .tile, .skin-picker, .split .media, .split .text, .lala-strip, .shots, .keys, .faq details, .final")) {
      el.classList.add("reveal");
      io.observe(el);
    }
  }
})();
