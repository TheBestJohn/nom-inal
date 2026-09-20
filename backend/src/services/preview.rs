//! The 1200×630 card a shared recipe shows in a link preview.
//!
//! Every chat app and every crawler wants an image, and wants it at a fixed
//! shape: Open Graph's `summary_large_image` is 1.91:1, which is 1200×630.
//! A recipe with photos already has one — its first photo, re-cropped — and a
//! recipe without photos would otherwise fall back to whatever the platform
//! invents, which is usually nothing. So one is drawn: the name, the serving
//! and calorie line, and the application's name, on the theme's green.
//!
//! Drawn with the `image` crate, which is already here for photo uploads, and
//! `ab_glyph` for the glyph outlines. The font is vendored beside this file
//! rather than read from the host: the runtime image is debian-slim with no
//! fonts in it at all, so a card drawn from a system font would come out
//! blank on the one machine it matters on.

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use image::imageops::FilterType;
use image::{ImageReader, Rgb, RgbImage};

use crate::error::ApiError;

/// Open Graph's large-image shape. Also what Twitter, Slack and iMessage
/// crop to, so matching it exactly means nobody else crops it.
pub const WIDTH: u32 = 1200;
pub const HEIGHT: u32 = 630;

/// DejaVu Sans Bold, vendored. See `LICENSE-DejaVu.txt` beside it.
const FONT: &[u8] = include_bytes!("../../assets/fonts/DejaVuSans-Bold.ttf");

/// `--primary` in light mode, oklch(0.52 0.09 158), which is the green the
/// application's buttons are.
const GREEN: Rgb<u8> = Rgb([0x35, 0x78, 0x55]);
/// `--primary-foreground`: what text on that green is.
const ON_GREEN: Rgb<u8> = Rgb([0xf7, 0xfb, 0xf9]);
/// `--accent-foreground` in dark mode — the same hue, dimmed, for the lines
/// that are not the recipe's name.
const DIM: Rgb<u8> = Rgb([0xce, 0xe4, 0xd7]);

const MARGIN: f32 = 80.0;
const NAME_SIZE: f32 = 76.0;
const NAME_LEADING: f32 = 92.0;
const NAME_MAX_LINES: usize = 3;
const SUB_SIZE: f32 = 36.0;
/// Space between the last line of the name and the line beneath it.
const SUB_GAP: f32 = 24.0;
const MARK_SIZE: f32 = 34.0;

/// A photo, cropped to the card's shape rather than letterboxed.
///
/// Cover rather than contain: a preview with bars down the sides looks like a
/// mistake in every client that shows it, and the middle of a photo of dinner
/// is the dinner.
pub fn from_photo(bytes: &[u8]) -> Result<Vec<u8>, ApiError> {
    let image = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("reading photo for preview: {e}")))?
        .decode()
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("decoding photo for preview: {e}")))?;

    // Scale so both edges cover, then take the middle.
    let (w, h) = (image.width().max(1), image.height().max(1));
    let scale = (WIDTH as f32 / w as f32).max(HEIGHT as f32 / h as f32);
    let scaled = image.resize(
        ((w as f32 * scale).ceil() as u32).max(WIDTH),
        ((h as f32 * scale).ceil() as u32).max(HEIGHT),
        FilterType::Lanczos3,
    );
    let x = (scaled.width() - WIDTH) / 2;
    let y = (scaled.height() - HEIGHT) / 2;
    // To RGB before encoding: a photo with an alpha channel would otherwise
    // produce a card that some clients composite against black.
    let cropped = image::imageops::crop_imm(&scaled, x, y, WIDTH, HEIGHT).to_image();

    encode(&image::DynamicImage::ImageRgba8(cropped).to_rgb8())
}

/// What the drawn card says.
pub struct Card<'a> {
    pub name: &'a str,
    pub servings: f64,
    pub calories_per_serving: f64,
}

impl Card<'_> {
    /// "4 servings · 320 kcal per serving".
    fn subtitle(&self) -> String {
        let servings = crate::domain::recipe::trim_float(self.servings);
        format!(
            "{servings} serving{} · {} kcal per serving",
            if self.servings == 1.0 { "" } else { "s" },
            self.calories_per_serving.round()
        )
    }
}

/// A card for a recipe with no photo of its own.
pub fn generated(card: &Card) -> Result<Vec<u8>, ApiError> {
    let font = FontRef::try_from_slice(FONT)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("preview font: {e}")))?;

    let mut canvas = RgbImage::from_pixel(WIDTH, HEIGHT, GREEN);

    let name_scale = font.as_scaled(PxScale::from(NAME_SIZE));
    let usable = WIDTH as f32 - 2.0 * MARGIN;
    let lines = wrap_lines(card.name, NAME_MAX_LINES, usable, &|text| {
        text_width(&name_scale, text)
    });

    // The name and its subtitle sit as one block, centred in the space above
    // the wordmark, so a one-line name does not hug the top of the card.
    let block = lines.len() as f32 * NAME_LEADING + SUB_GAP + SUB_SIZE;
    let top = (HEIGHT as f32 - MARK_SIZE - MARGIN - block) / 2.0;

    let mut baseline = top + NAME_SIZE;
    for line in &lines {
        draw_text(
            &mut canvas,
            &font,
            NAME_SIZE,
            MARGIN,
            baseline,
            line,
            ON_GREEN,
        );
        baseline += NAME_LEADING;
    }
    // `baseline` is now one leading past the last line drawn; the subtitle
    // hangs a fixed gap below that line rather than below the whole block, so
    // a one-line name and a three-line name space the same.
    let sub_baseline = baseline - NAME_LEADING + SUB_GAP + SUB_SIZE;

    let sub_scale = font.as_scaled(PxScale::from(SUB_SIZE));
    let subtitle = card.subtitle();
    let subtitle = wrap_lines(&subtitle, 1, usable, &|text| text_width(&sub_scale, text))
        .pop()
        .unwrap_or_default();
    draw_text(
        &mut canvas,
        &font,
        SUB_SIZE,
        MARGIN,
        sub_baseline,
        &subtitle,
        DIM,
    );

    draw_text(
        &mut canvas,
        &font,
        MARK_SIZE,
        MARGIN,
        HEIGHT as f32 - MARGIN,
        "nom-inal",
        DIM,
    );

    encode(&canvas)
}

fn encode(image: &RgbImage) -> Result<Vec<u8>, ApiError> {
    let mut out = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("encoding preview: {e}")))?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

fn text_width<F: Font, S: ScaleFont<F>>(scaled: &S, text: &str) -> f32 {
    let mut width = 0.0;
    let mut previous = None;
    for ch in text.chars() {
        let glyph = scaled.glyph_id(ch);
        if let Some(prev) = previous {
            width += scaled.kern(prev, glyph);
        }
        width += scaled.h_advance(glyph);
        previous = Some(glyph);
    }
    width
}

/// Break `text` into at most `max_lines` lines no wider than `max_width`,
/// ellipsising the last one when there is more text than fits.
///
/// `width_of` is passed in rather than measured here so the rule can be
/// tested without a font: a recipe name that overflows the card is a layout
/// bug, and layout bugs are only visible in a picture unless the thing that
/// decides the layout is a function with an assertion on it.
pub fn wrap_lines(
    text: &str,
    max_lines: usize,
    max_width: f32,
    width_of: &dyn Fn(&str) -> f32,
) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();

    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if width_of(&candidate) <= max_width {
            current = candidate;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        // A single word wider than the card — a URL, a German compound — is
        // broken rather than allowed to run off the edge.
        let mut rest = word.to_string();
        while width_of(&rest) > max_width && lines.len() <= max_lines {
            let head = fit(&rest, max_width, width_of);
            if head.is_empty() {
                break;
            }
            rest = rest.chars().skip(head.chars().count()).collect();
            lines.push(head);
        }
        current = rest;
    }
    if !current.is_empty() {
        lines.push(current);
    }

    if lines.len() <= max_lines {
        return lines;
    }
    lines.truncate(max_lines);
    if let Some(last) = lines.last_mut() {
        *last = ellipsise(last, max_width, width_of);
    }
    lines
}

/// The longest prefix of `text` that fits.
fn fit(text: &str, max_width: f32, width_of: &dyn Fn(&str) -> f32) -> String {
    let mut head = String::new();
    for ch in text.chars() {
        let next = format!("{head}{ch}");
        if width_of(&next) > max_width {
            break;
        }
        head = next;
    }
    head
}

/// `text` with an ellipsis, shortened until the pair of them fit.
fn ellipsise(text: &str, max_width: f32, width_of: &dyn Fn(&str) -> f32) -> String {
    let mut chars: Vec<char> = text.trim_end().chars().collect();
    loop {
        let candidate: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
        if width_of(&candidate) <= max_width || chars.is_empty() {
            return candidate;
        }
        chars.pop();
    }
}

fn draw_text(
    canvas: &mut RgbImage,
    font: &FontRef<'_>,
    size: f32,
    x: f32,
    baseline: f32,
    text: &str,
    colour: Rgb<u8>,
) {
    let scaled = font.as_scaled(PxScale::from(size));
    let mut caret = x;
    let mut previous = None;

    for ch in text.chars() {
        let id = scaled.glyph_id(ch);
        if let Some(prev) = previous {
            caret += scaled.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(PxScale::from(size), point(caret, baseline));
        caret += scaled.h_advance(id);
        previous = Some(id);

        let Some(outlined) = font.outline_glyph(glyph) else {
            continue;
        };
        let bounds = outlined.px_bounds();
        outlined.draw(|gx, gy, coverage| {
            let px = bounds.min.x as i32 + gx as i32;
            let py = bounds.min.y as i32 + gy as i32;
            if px < 0 || py < 0 || px >= WIDTH as i32 || py >= HEIGHT as i32 {
                return;
            }
            let under = *canvas.get_pixel(px as u32, py as u32);
            canvas.put_pixel(px as u32, py as u32, blend(under, colour, coverage));
        });
    }
}

fn blend(under: Rgb<u8>, over: Rgb<u8>, coverage: f32) -> Rgb<u8> {
    let a = coverage.clamp(0.0, 1.0);
    let mix = |u: u8, o: u8| (u as f32 * (1.0 - a) + o as f32 * a).round() as u8;
    Rgb([
        mix(under.0[0], over.0[0]),
        mix(under.0[1], over.0[1]),
        mix(under.0[2], over.0[2]),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for a font: every character one unit wide. The wrapping
    /// rule is what is under test, not the metrics of DejaVu.
    fn monospace(text: &str) -> f32 {
        text.chars().count() as f32
    }

    #[test]
    fn a_short_name_is_one_line() {
        assert_eq!(
            wrap_lines("Roast chicken", 3, 20.0, &monospace),
            ["Roast chicken"]
        );
    }

    #[test]
    fn a_long_name_wraps_on_word_boundaries() {
        assert_eq!(
            wrap_lines(
                "Slow roasted tomato and red pepper soup",
                3,
                20.0,
                &monospace
            ),
            ["Slow roasted tomato", "and red pepper soup"]
        );
    }

    #[test]
    fn a_name_longer_than_the_card_is_ellipsised_rather_than_overflowing() {
        let lines = wrap_lines(
            "Slow roasted tomato and red pepper soup with basil oil and a swirl of cream on top",
            2,
            20.0,
            &monospace,
        );
        assert_eq!(lines.len(), 2);
        assert!(lines.last().unwrap().ends_with('…'), "{lines:?}");
        for line in &lines {
            assert!(monospace(line) <= 20.0, "{line:?} overflows");
        }
    }

    #[test]
    fn one_unbroken_word_is_broken_rather_than_running_off_the_edge() {
        let lines = wrap_lines(&"a".repeat(75), 3, 20.0, &monospace);
        for line in &lines {
            assert!(monospace(line) <= 20.0, "{line:?} overflows");
        }
        assert_eq!(lines.len(), 3);
        assert!(lines.last().unwrap().ends_with('…'));
    }

    #[test]
    fn an_empty_name_produces_no_lines_rather_than_an_empty_one() {
        assert!(wrap_lines("   ", 3, 20.0, &monospace).is_empty());
    }

    #[test]
    fn the_real_font_keeps_a_long_name_inside_the_card() {
        let font = FontRef::try_from_slice(FONT).unwrap();
        let scaled = font.as_scaled(PxScale::from(NAME_SIZE));
        let usable = WIDTH as f32 - 2.0 * MARGIN;
        let lines = wrap_lines(
            "Grandmother's slow-roasted tomato and red pepper soup with basil oil",
            NAME_MAX_LINES,
            usable,
            &|text| text_width(&scaled, text),
        );
        assert!(!lines.is_empty());
        assert!(lines.len() <= NAME_MAX_LINES);
        for line in &lines {
            assert!(text_width(&scaled, line) <= usable, "{line:?} overflows");
        }
    }

    #[test]
    fn a_generated_card_is_a_png_of_the_right_shape() {
        let png = generated(&Card {
            name: "Pierogi Ruskie",
            servings: 4.0,
            calories_per_serving: 320.0,
        })
        .unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (WIDTH, HEIGHT));
    }

    /// The card has to have ink on it. A font that failed to load, or a
    /// layout that put the text off the canvas, would otherwise pass every
    /// other assertion here.
    #[test]
    fn a_generated_card_has_text_drawn_on_it() {
        let png = generated(&Card {
            name: "Pierogi Ruskie",
            servings: 4.0,
            calories_per_serving: 320.0,
        })
        .unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgb8();
        let inked = image.pixels().filter(|p| **p != GREEN).count();
        assert!(
            inked > 5_000,
            "only {inked} pixels differ from the background"
        );
    }

    #[test]
    fn a_photo_is_cropped_to_cover_rather_than_squashed() {
        let tall = image::RgbImage::from_fn(600, 1800, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, 90])
        });
        let mut source = Vec::new();
        image::DynamicImage::ImageRgb8(tall)
            .write_to(
                &mut std::io::Cursor::new(&mut source),
                image::ImageFormat::Png,
            )
            .unwrap();

        let png = from_photo(&source).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (WIDTH, HEIGHT));
    }

    #[test]
    fn a_file_that_is_not_an_image_does_not_produce_a_card() {
        assert!(from_photo(b"not a photo at all").is_err());
    }
}
