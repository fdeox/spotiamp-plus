//! A GEN.BMP for a classic skin that has none (most 2.x skins: it came with
//! Winamp 5's embedded windows), made from the skin's playlist frame
//! (PLEDIT.BMP), so Spotiamp+'s windows (the Library, Lyrics, the
//! visualizer…) look like the skin's playlist instead of Winamp's default
//! blue. The pieces sit where Winamp's GEN.BMP has them (genframe.js draws
//! from it); the title's letters are Winamp's own shapes in a colour that
//! reads on the skin's titlebar.

use std::sync::LazyLock;

use image::{Rgb, RgbImage};

/// GEN.BMP's "nothing here" colour: also what ends each letter
const SEP: [u8; 3] = [0, 198, 255];

/// Winamp's base skin's GEN.BMP, for its letters' shapes
static BASE_GEN: LazyLock<Option<RgbImage>> = LazyLock::new(|| {
    image::load_from_memory(include_bytes!("../../src/static/assets/skins/base-2.91/GEN.BMP"))
        .ok()
        .map(|img| img.to_rgb8())
});

/// [x, width] of A-Z on the base sheet's letter rows (88 and 96)
static LETTERS: LazyLock<Vec<(u32, u32)>> = LazyLock::new(|| {
    let Some(base) = BASE_GEN.as_ref() else {
        return Vec::new();
    };
    let mut cells = Vec::with_capacity(26);
    let mut x = 1;
    for _ in 0..26 {
        let mut end = x;
        // (the sheet ends in black right after its Z)
        while end < base.width() && base.get_pixel(end, 88).0 != SEP && base.get_pixel(end, 88).0 != [0, 0, 0] {
            end += 1;
        }
        cells.push((x, end - x));
        x = end + 1;
    }
    cells
});

type Px = [u8; 3];

fn diff(a: Px, b: Px) -> u32 {
    (0..3).map(|k| (a[k] as i32 - b[k] as i32).unsigned_abs()).sum()
}

fn lum(c: Px) -> f64 {
    0.299 * c[0] as f64 + 0.587 * c[1] as f64 + 0.114 * c[2] as f64
}

/// The biggest difference between two columns (or rows), pixel by pixel
fn line_dist(a: &[Px], b: &[Px]) -> u32 {
    a.iter().zip(b).map(|(p, q)| diff(*p, *q)).max().unwrap_or(0)
}

fn column(img: &RgbImage, x: u32, y0: u32, h: u32) -> Vec<Px> {
    (y0..y0 + h).map(|y| img.get_pixel(x, y).0).collect()
}

fn row(img: &RgbImage, y: u32, x0: u32, w: u32) -> Vec<Px> {
    (x0..x0 + w).map(|x| img.get_pixel(x, y).0).collect()
}

/// Which of these lines to keep so there are `n`: the one most like the
/// next goes, again and again, never the first or the last (a border
/// narrowed without losing its edges)
fn keep_lines(lines: &[Vec<Px>], n: usize) -> Vec<usize> {
    let mut keep: Vec<usize> = (0..lines.len()).collect();
    while keep.len() > n && keep.len() > 2 {
        let mut best = (u64::MAX, 1);
        for i in 1..keep.len() - 1 {
            let d: u64 = lines[keep[i]].iter().zip(&lines[keep[i + 1]]).map(|(p, q)| diff(*p, *q) as u64).sum();
            if d < best.0 {
                best = (d, i);
            }
        }
        keep.remove(best.1);
    }
    keep
}

/// A picture of the given columns and rows of `img` (offsets from x0, y0)
fn pick(img: &RgbImage, x0: u32, y0: u32, xs: &[usize], ys: &[usize]) -> RgbImage {
    let mut out = RgbImage::new(xs.len() as u32, ys.len() as u32);
    for (i, x) in xs.iter().enumerate() {
        for (j, y) in ys.iter().enumerate() {
            out.put_pixel(i as u32, j as u32, *img.get_pixel(x0 + *x as u32, y0 + *y as u32));
        }
    }
    out
}

fn crop(img: &RgbImage, x: u32, y: u32, w: u32, h: u32) -> RgbImage {
    image::imageops::crop_imm(img, x, y, w, h).to_image()
}

fn paste(dst: &mut RgbImage, src: &RgbImage, x: u32, y: u32) {
    image::imageops::replace(dst, src, x as i64, y as i64);
}

/// GEN.BMP made from PLEDIT.BMP (`current`, `normal`: the playlist's text
/// colours, PLEDIT.TXT), or None when the sheet is too small to hold the
/// playlist's frame.
pub fn from_pledit(p: &RgbImage, current: Option<Px>, normal: Option<Px>) -> Option<RgbImage> {
    let base = BASE_GEN.as_ref()?;
    if p.width() < 276 || p.height() < 110 || LETTERS.len() != 26 {
        return None;
    }
    let mut g = RgbImage::from_pixel(194, 109, Rgb(SEP));
    let mut ink_on: Option<Px> = None;
    // the titlebar, focused (row 0) and not (21), as the playlist's
    for s in [0u32, 21] {
        let tl = crop(p, 0, s, 25, 20);
        let fill = crop(p, 127, s, 25, 20);
        let mut tr = crop(p, 153, s, 25, 20);
        // the playlist's shade button (4..13 in the corner) goes: the column
        // before it carried across (the close button stays where GEN.BMP's is)
        for y in 2..13 {
            let c = *tr.get_pixel(3, y);
            for x in 4..14 {
                tr.put_pixel(x, y, c);
            }
        }
        // the title area (26..125): the plate is its plain column nearest
        // the middle (one like its neighbours, not the bars of the fill),
        // else each row's middle value across its middle
        let tcols: Vec<Vec<Px>> = (26..126).map(|x| column(p, x, s, 20)).collect();
        let fcols: Vec<Vec<Px>> = (127..152).map(|x| column(p, x, s, 20)).collect();
        let plain: Vec<usize> = (1..99)
            .filter(|&x| {
                line_dist(&tcols[x], &tcols[x - 1]) <= 12
                    && line_dist(&tcols[x], &tcols[x + 1]) <= 12
                    && fcols.iter().all(|f| line_dist(&tcols[x], f) > 24)
            })
            .collect();
        let plate: Vec<Px> = match plain.iter().min_by_key(|&&x| ((x as i32 * 2 - 99).abs(), x)) {
            Some(&x) => tcols[x].clone(),
            None => (0..20)
                .map(|y| {
                    let mut c = [0u8; 3];
                    for (k, ck) in c.iter_mut().enumerate() {
                        let mut v: Vec<u8> = (25..75).map(|x| tcols[x][y as usize][k]).collect();
                        v.sort_unstable();
                        *ck = v[v.len() / 2];
                    }
                    c
                })
                .collect(),
        };
        let mut mid = RgbImage::new(25, 20);
        for (_, y, px) in mid.enumerate_pixels_mut() {
            *px = Rgb(plate[y as usize]);
        }
        // the title area's own ends beside the plate, when they're only its
        // edges: the plate from where it starts to the middle, and the two
        // ends mirroring each other (a playlist's own title in them doesn't)
        let is_plate = |x: usize| (0..20).all(|y| diff(tcols[x][y], plate[y]) <= 24);
        let inner = |xs: &mut dyn Iterator<Item = usize>| -> Option<usize> {
            let flags: Vec<bool> = xs.map(is_plate).collect();
            let k = flags.iter().position(|f| *f)?;
            flags[k..].iter().all(|f| *f).then_some(k)
        };
        let ends = match (inner(&mut (0..25)), inner(&mut (75..100).rev())) {
            (Some(kl), Some(kr)) => {
                let k = kl.max(kr);
                let same = (0..20).flat_map(|y| (0..k).map(move |x| (x, y))).filter(|&(x, y)| diff(tcols[x][y], tcols[99 - x][y]) <= 48).count();
                k == 0 || same * 10 >= 20 * k * 9
            }
            _ => false,
        };
        let (lcap, rcap) = if ends { (crop(p, 26, s, 25, 20), crop(p, 101, s, 25, 20)) } else { (fill.clone(), fill.clone()) };
        for (x, piece) in [(0, &tl), (26, &lcap), (52, &mid), (78, &rcap), (104, &fill), (130, &tr)] {
            paste(&mut g, piece, x, s);
        }

        // the title's colour: the playlist's own when it reads on the plate
        // (where the letters are, rows 5..10), else near-black or near-white
        let reads = |c: Px| (5..11).map(|y| crate::wsz::contrast(c, plate[y]).min(7.0)).sum::<f64>() / 6.0;
        let band: Px = {
            let mut c = [0u32; 3];
            for px in &plate[5..11] {
                for k in 0..3 {
                    c[k] += px[k] as u32;
                }
            }
            c.map(|v| (v / 6) as u8)
        };
        let ink = match ink_on {
            None => {
                let own = [current, normal].into_iter().flatten().find(|c| reads(*c) >= 3.0);
                let ink = own.unwrap_or_else(|| {
                    let (dark, light) = ([16, 16, 20], [240, 242, 248]);
                    if reads(dark) >= reads(light) { dark } else { light }
                });
                ink_on = Some(ink);
                ink
            }
            // not focused: halfway to the plate
            Some(ink) => std::array::from_fn(|k| ((ink[k] as u32 * 55 + band[k] as u32 * 45) / 100) as u8),
        };
        // the letters: Winamp's shapes (white, or grey when not focused, on
        // navy) in that colour on the plate
        let ly = if s == 0 { 88 } else { 96 };
        let bg_l = lum([41, 41, 64]);
        let ink_l = if s == 0 { 255.0 } else { lum([148, 148, 160]) };
        for &(x, w) in LETTERS.iter() {
            for k in 0..7u32 {
                let back = plate[(4 + k) as usize];
                for i in 0..w {
                    let l = lum(base.get_pixel(x + i, ly + k).0);
                    let t = ((l - bg_l) / (ink_l - bg_l)).clamp(0.0, 1.0);
                    let mut c: Px = std::array::from_fn(|j| (back[j] as f64 * (1.0 - t) + ink[j] as f64 * t).round() as u8);
                    if c == SEP {
                        c[2] -= 1;
                    }
                    g.put_pixel(x + i, ly + k, Rgb(c));
                }
            }
        }
    }
    // the close button pressed
    paste(&mut g, &crop(p, 52, 42, 9, 9), 148, 42);
    // the sides: the playlist's, narrowed to 11 and 8 (the right one loses
    // its scrollbar's groove)
    let lcols: Vec<Vec<Px>> = (0..12).map(|x| column(p, x, 42, 29)).collect();
    let rcols: Vec<Vec<Px>> = (31..51).map(|x| column(p, x, 42, 29)).collect();
    let lx = keep_lines(&lcols, 11);
    let rx = keep_lines(&rcols, 8);
    let all29: Vec<usize> = (0..29).collect();
    paste(&mut g, &pick(p, 0, 42, &lx, &all29), 127, 42);
    paste(&mut g, &pick(p, 31, 42, &rx, &all29), 139, 42);
    paste(&mut g, &pick(p, 0, 42, &lx, &all29[..24]), 158, 42);
    paste(&mut g, &pick(p, 31, 42, &rx, &all29[..24]), 170, 42);
    // the bottom: 14 of the 38 rows of the playlist's plain bottom, with the
    // outer columns of its corners
    let frows: Vec<Vec<Px>> = (0..38).map(|y| row(p, y, 179, 25)).collect();
    let ry = keep_lines(&frows, 14);
    let all25: Vec<usize> = (0..25).collect();
    let fb = pick(p, 179, 0, &all25, &ry);
    let mut bl = RgbImage::new(125, 14);
    let mut br = RgbImage::new(125, 14);
    for x in (0..125).step_by(25) {
        paste(&mut bl, &fb, x, 0);
        paste(&mut br, &fb, x, 0);
    }
    paste(&mut bl, &pick(p, 0, 72, &lx, &ry), 0, 0);
    paste(&mut br, &pick(p, 256, 72, &rx, &ry), 117, 0);
    paste(&mut g, &bl, 0, 42);
    paste(&mut g, &br, 0, 57);
    paste(&mut g, &fb, 127, 72);
    Some(g)
}

/// Kept for the tests: how many distinct letter pictures a sheet's row has
#[cfg(test)]
fn letter_pictures(sheet: &RgbImage, y: u32) -> usize {
    let mut seen = std::collections::HashSet::new();
    for &(x, w) in LETTERS.iter() {
        let pic: Vec<Px> = (y..y + 7).flat_map(|yy| (x..x + w).map(move |xx| (xx, yy))).map(|(xx, yy)| sheet.get_pixel(xx, yy).0).collect();
        seen.insert(pic);
    }
    seen.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_pledit() -> RgbImage {
        image::load_from_memory(include_bytes!("../../src/static/assets/skins/base-2.91/PLEDIT.BMP")).unwrap().to_rgb8()
    }

    #[test]
    fn the_base_letters_are_read() {
        assert_eq!(LETTERS.len(), 26);
        assert!(LETTERS.iter().all(|&(_, w)| (3..=9).contains(&w)), "{:?}", *LETTERS);
        // (the RLE sheet decoded as Windows does: the frame's corner is navy)
        let base = BASE_GEN.as_ref().unwrap();
        assert_ne!(base.get_pixel(5, 5).0, SEP);
    }

    #[test]
    fn a_gen_sheet_is_made_from_the_playlist() {
        let p = base_pledit();
        let g = from_pledit(&p, Some([255, 255, 255]), Some([0, 255, 0])).unwrap();
        assert_eq!((g.width(), g.height()), (194, 109));
        // the corners are the playlist's
        assert_eq!(crop(&g, 0, 0, 25, 20), crop(&p, 0, 0, 25, 20));
        assert_eq!(crop(&g, 104, 21, 25, 20), crop(&p, 127, 21, 25, 20));
        // the letters are there, all different, and end where they should
        assert!(letter_pictures(&g, 88) >= 20);
        assert_eq!(crate::wsz::gen_letters(&g, 88), Some(LETTERS.iter().map(|(x, w)| format!("{x}:{w}")).collect::<Vec<_>>().join(" ")));
        // on the base skin's navy titlebar the title is in its white
        let (x, w) = LETTERS[0];
        let row: Vec<Px> = (x..x + w).map(|xx| g.get_pixel(xx, 90).0).collect();
        assert!(row.iter().any(|c| lum(*c) > 200.0), "{row:?}");
    }

    #[test]
    fn a_small_playlist_sheet_makes_none() {
        assert!(from_pledit(&RgbImage::new(200, 100), None, None).is_none());
    }
}
