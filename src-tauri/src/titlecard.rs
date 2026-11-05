//! Plain text title cards in SteamDrop's style, used when no real artwork is found.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};

static FONT: &[u8] = include_bytes!("../../ui/fonts/ChakraPetch-Bold.ttf");

pub enum Kind {
    /// Background with the title on it (portrait and wide covers).
    Cover,
    /// Background only; Steam lays the logo over it.
    Hero,
    /// Title only, on a transparent background.
    Logo,
}

const TOP: [f32; 3] = [27.0, 35.0, 54.0];
const BOTTOM: [f32; 3] = [11.0, 14.0, 22.0];
const ACCENT: [f32; 3] = [92.0, 200.0, 255.0];

pub fn render(title: &str, w: u32, h: u32, kind: Kind) -> Vec<u8> {
    let mut px = vec![0f32; (w * h * 4) as usize];
    if !matches!(kind, Kind::Logo) {
        for y in 0..h {
            let t = y as f32 / h as f32;
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                for c in 0..3 {
                    px[i + c] = TOP[c] + (BOTTOM[c] - TOP[c]) * t;
                }
                px[i + 3] = 255.0;
            }
        }
        // A thin accent stripe along the bottom edge.
        let stripe = (h / 60).max(3);
        fill(&mut px, w, 0, h - stripe, w, stripe, ACCENT);
    }
    if !matches!(kind, Kind::Hero) {
        draw_title(&mut px, w, h, title);
    }
    encode(&px, w, h)
}

fn fill(px: &mut [f32], w: u32, x0: u32, y0: u32, fw: u32, fh: u32, rgb: [f32; 3]) {
    for y in y0..y0 + fh {
        for x in x0..x0 + fw {
            let i = ((y * w + x) * 4) as usize;
            px[i..i + 3].copy_from_slice(&rgb);
            px[i + 3] = 255.0;
        }
    }
}

/// Word-wraps the title and picks the largest size that fits in the middle of the image.
fn draw_title(px: &mut [f32], w: u32, h: u32, title: &str) {
    let font = FontRef::try_from_slice(FONT).expect("bundled font");
    let (max_w, max_h) = (w as f32 * 0.84, h as f32 * 0.62);
    let mut size = h as f32 * 0.3;
    let lines = loop {
        let sf = font.as_scaled(PxScale::from(size));
        let lines = wrap(&sf, title, max_w);
        let height = lines.len() as f32 * sf.height() * 1.05;
        let widest = lines.iter().map(|l| width(&sf, l)).fold(0.0, f32::max);
        if (height <= max_h && widest <= max_w) || size < 10.0 {
            break lines;
        }
        size *= 0.92;
    };
    let sf = font.as_scaled(PxScale::from(size));
    let line_h = sf.height() * 1.05;
    let mut y = (h as f32 - line_h * lines.len() as f32) / 2.0 + sf.ascent();
    for line in &lines {
        let mut x = (w as f32 - width(&sf, line)) / 2.0;
        let mut prev = None;
        for ch in line.chars() {
            let id = font.glyph_id(ch);
            if let Some(p) = prev {
                x += sf.kern(p, id);
            }
            let glyph = id.with_scale_and_position(size, ab_glyph::point(x, y));
            if let Some(outline) = font.outline_glyph(glyph) {
                let b = outline.px_bounds();
                outline.draw(|gx, gy, cov| {
                    let (x, y) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
                        let i = ((y as u32 * w + x as u32) * 4) as usize;
                        let a = cov.clamp(0.0, 1.0);
                        for c in 0..3 {
                            px[i + c] = px[i + c] * (1.0 - a) + 245.0 * a;
                        }
                        px[i + 3] = px[i + 3] + (255.0 - px[i + 3]) * a;
                    }
                });
            }
            x += sf.h_advance(id);
            prev = Some(id);
        }
        y += line_h;
    }
}

fn width<F: Font>(sf: &ab_glyph::PxScaleFont<&F>, text: &str) -> f32 {
    let mut prev = None;
    let mut x = 0.0;
    for ch in text.chars() {
        let id = sf.glyph_id(ch);
        if let Some(p) = prev {
            x += sf.kern(p, id);
        }
        x += sf.h_advance(id);
        prev = Some(id);
    }
    x
}

fn wrap<F: Font>(sf: &ab_glyph::PxScaleFont<&F>, text: &str, max_w: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(last) if width(sf, &format!("{last} {word}")) <= max_w => {
                last.push(' ');
                last.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    if lines.is_empty() {
        lines.push(text.to_string());
    }
    lines
}

fn encode(px: &[f32], w: u32, h: u32) -> Vec<u8> {
    let bytes: Vec<u8> = px.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect();
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().expect("png header");
    writer.write_image_data(&bytes).expect("png data");
    writer.finish().expect("png end");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_valid_pngs() {
        for (kind, w, h) in [(Kind::Cover, 600, 900), (Kind::Cover, 920, 430), (Kind::Hero, 1920, 620), (Kind::Logo, 1280, 720)] {
            let png = render("A Rather Long Game Title: The Sequel", w, h, kind);
            assert_eq!(&png[1..4], b"PNG");
            let dec = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
            assert_eq!((dec.info().width, dec.info().height), (w, h));
        }
    }
}
