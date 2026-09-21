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
/// Space between the serving line and the macros beneath it. Tighter than
/// `SUB_GAP`: the two are one statement about a serving.
const MACRO_GAP: f32 = 14.0;
/// The name over a photo is smaller than on a drawn card: the picture is
/// doing most of the work and the text is a caption on it.
const PHOTO_NAME_SIZE: f32 = 54.0;
const PHOTO_NAME_LEADING: f32 = 66.0;
const PHOTO_NAME_MAX_LINES: usize = 2;
/// How dark the foot of a photo is taken, under the text. Gentle, because
/// the outline around each letter is what makes the caption legible; this
/// only settles the area down so the text does not sit on a busy highlight.
const SCRIM_MAX: f32 = 0.42;
/// Clear air between the top of the text and where the darkening stops being
/// solid, so no ascender pokes out of its floor.
const SCRIM_PAD: f32 = 18.0;
/// The band above that, over which the darkening fades away to nothing.
const SCRIM_FADE: f32 = 150.0;
/// Outline thickness as a fraction of the type size, so the edge stays in
/// proportion whether it is around the name or the figures.
const OUTLINE_DIVISOR: f32 = 16.0;
/// What the outline is drawn in: near-black, not pure, which reads as a
/// shadow rather than as a sticker.
const OUTLINE: Rgb<u8> = Rgb([0x10, 0x14, 0x12]);
/// Plain white over a photo: the dimmed green reads as a colour cast when the
/// thing behind it is a photograph rather than the theme's green.
const ON_PHOTO: Rgb<u8> = Rgb([0xff, 0xff, 0xff]);

/// A photo, cropped to the card's shape rather than letterboxed.
///
/// Cover rather than contain: a preview with bars down the sides looks like a
/// mistake in every client that shows it, and the middle of a photo of dinner
/// is the dinner.
pub fn from_photo(bytes: &[u8], card: &Card) -> Result<Vec<u8>, ApiError> {
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
    let mut canvas = image::DynamicImage::ImageRgba8(cropped).to_rgb8();

    // A photo says what the dish looks like and nothing about what is in it,
    // so the same lines the drawn card carries go over the foot of it.
    caption(&mut canvas, card)?;

    encode(&canvas)
}

/// Darken the foot of a photo and write the recipe's name and figures on it.
///
/// The text is laid out first and the darkening is cut to fit it, rather than
/// the other way round: a fixed-height gradient leaves the top line sitting on
/// whatever the photograph happens to be, which on a bright one is white text
/// on near-white. Every line sits on the same fully-darkened floor, and the
/// darkening fades out above it so it reads as light falling off rather than
/// as a bar laid across the picture.
fn caption(canvas: &mut RgbImage, card: &Card) -> Result<(), ApiError> {
    let font = FontRef::try_from_slice(FONT)
        .map_err(|e| ApiError::Internal(anyhow::anyhow!("preview font: {e}")))?;

    let name_scale = font.as_scaled(PxScale::from(PHOTO_NAME_SIZE));
    let sub_scale = font.as_scaled(PxScale::from(SUB_SIZE));
    let usable = WIDTH as f32 - 2.0 * MARGIN;
    let lines = wrap_lines(card.name, PHOTO_NAME_MAX_LINES, usable, &|text| {
        text_width(&name_scale, text)
    });

    // Baselines from the bottom up: the last line sits a margin off the foot.
    let macros_baseline = HEIGHT as f32 - MARGIN;
    let sub_baseline = macros_baseline - MACRO_GAP - SUB_SIZE;
    let last_name = sub_baseline - SUB_GAP - SUB_SIZE;
    let first_name = last_name - (lines.len() as f32 - 1.0) * PHOTO_NAME_LEADING;
    // The top of the tallest thing drawn, not its baseline.
    let ink_top = first_name - PHOTO_NAME_SIZE;

    let solid_from = (ink_top - SCRIM_PAD).max(0.0);
    let fade_from = (solid_from - SCRIM_FADE).max(0.0);
    for y in fade_from as u32..HEIGHT {
        let alpha = if y as f32 >= solid_from {
            SCRIM_MAX
        } else {
            let t = (y as f32 - fade_from) / (solid_from - fade_from).max(1.0);
            SCRIM_MAX * t * t
        };
        for x in 0..WIDTH {
            let px = canvas.get_pixel_mut(x, y);
            for c in 0..3 {
                px.0[c] = (px.0[c] as f32 * (1.0 - alpha)) as u8;
            }
        }
    }

    let mut baseline = first_name;
    for line in &lines {
        draw_text(
            canvas,
            &font,
            Style::outlined(PHOTO_NAME_SIZE, ON_PHOTO),
            MARGIN,
            baseline,
            line,
        );
        baseline += PHOTO_NAME_LEADING;
    }

    for (text, at) in [
        (card.subtitle(), sub_baseline),
        (card.macros(), macros_baseline),
    ] {
        let line = wrap_lines(&text, 1, usable, &|t| text_width(&sub_scale, t))
            .pop()
            .unwrap_or_default();
        draw_text(
            canvas,
            &font,
            Style::outlined(SUB_SIZE, ON_PHOTO),
            MARGIN,
            at,
            &line,
        );
    }

    Ok(())
}

/// What a card says. Every figure is per serving, which is the number a
/// person reading a link is deciding about — a whole recipe's totals mean
/// nothing without knowing how many it feeds.
pub struct Card<'a> {
    pub name: &'a str,
    pub servings: f64,
    pub calories_per_serving: f64,
    pub protein_per_serving: f64,
    pub carbs_per_serving: f64,
    pub fat_per_serving: f64,
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

    /// "32 g protein · 28 g carbs · 9 g fat".
    ///
    /// Written out rather than in the app's own P/C/F shorthand: this card is
    /// read in a chat window, at a glance, by people who have never seen the
    /// application.
    fn macros(&self) -> String {
        format!(
            "{} g protein · {} g carbs · {} g fat",
            self.protein_per_serving.round(),
            self.carbs_per_serving.round(),
            self.fat_per_serving.round(),
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
    let block = lines.len() as f32 * NAME_LEADING + SUB_GAP + SUB_SIZE + MACRO_GAP + SUB_SIZE;
    let top = (HEIGHT as f32 - MARK_SIZE - MARGIN - block) / 2.0;

    let mut baseline = top + NAME_SIZE;
    for line in &lines {
        draw_text(
            &mut canvas,
            &font,
            Style::plain(NAME_SIZE, ON_GREEN),
            MARGIN,
            baseline,
            line,
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
        Style::plain(SUB_SIZE, DIM),
        MARGIN,
        sub_baseline,
        &subtitle,
    );

    // What is in a serving, under what a serving is. A preview that gives a
    // calorie figure and nothing else states the price without the goods.
    let macros = card.macros();
    let macros = wrap_lines(&macros, 1, usable, &|text| text_width(&sub_scale, text))
        .pop()
        .unwrap_or_default();
    draw_text(
        &mut canvas,
        &font,
        Style::plain(SUB_SIZE, DIM),
        MARGIN,
        sub_baseline + MACRO_GAP + SUB_SIZE,
        &macros,
    );

    draw_text(
        &mut canvas,
        &font,
        Style::plain(MARK_SIZE, DIM),
        MARGIN,
        HEIGHT as f32 - MARGIN,
        "nom-inal",
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

/// Walk a line of text, handing every covered pixel and its coverage to `f`.
///
/// Shared by the plain and the outlined draw so the two cannot disagree about
/// where a glyph sits: an outline offset by a kerning rule the fill does not
/// use would show as a shadow down one side of the word.
fn for_each_glyph_pixel(
    font: &FontRef<'_>,
    size: f32,
    x: f32,
    baseline: f32,
    text: &str,
    f: &mut impl FnMut(i32, i32, f32),
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
            f(
                bounds.min.x as i32 + gx as i32,
                bounds.min.y as i32 + gy as i32,
                coverage,
            );
        });
    }
}

/// How a line of text is drawn: its size, its colour, and whether it carries
/// an edge. Together rather than as loose arguments, so a call site cannot
/// pair the name's size with the figures' colour.
#[derive(Clone, Copy)]
struct Style {
    size: f32,
    fill: Rgb<u8>,
    /// Set over a photograph, where contrast cannot be assumed.
    edge: Option<Rgb<u8>>,
}

impl Style {
    fn plain(size: f32, fill: Rgb<u8>) -> Self {
        Self {
            size,
            fill,
            edge: None,
        }
    }

    fn outlined(size: f32, fill: Rgb<u8>) -> Self {
        Self {
            size,
            fill,
            edge: Some(OUTLINE),
        }
    }
}

fn draw_text(
    canvas: &mut RgbImage,
    font: &FontRef<'_>,
    style: Style,
    x: f32,
    baseline: f32,
    text: &str,
) {
    if style.edge.is_some() {
        draw_text_outlined(canvas, font, style, x, baseline, text);
        return;
    }
    let (size, colour) = (style.size, style.fill);
    for_each_glyph_pixel(font, size, x, baseline, text, &mut |px, py, coverage| {
        if px < 0 || py < 0 || px >= WIDTH as i32 || py >= HEIGHT as i32 {
            return;
        }
        let under = *canvas.get_pixel(px as u32, py as u32);
        canvas.put_pixel(px as u32, py as u32, blend(under, colour, coverage));
    });
}

/// The same text with a dark edge drawn around it.
///
/// Over a photograph, contrast cannot be assumed: a white word crossing from
/// a shadow onto a highlight is legible for half its length. An outline gives
/// every letter its own contrast, whatever it happens to be lying on, which
/// is what lets the darkening underneath be gentle enough to still show the
/// food.
///
/// The edge is the glyph coverage grown by `radius` — a dilation — rather
/// than the word stamped at offsets around itself. Stamping leaves the
/// corners thin and the overlaps dense, and at these sizes that reads as a
/// bad drop shadow.
fn draw_text_outlined(
    canvas: &mut RgbImage,
    font: &FontRef<'_>,
    style: Style,
    x: f32,
    baseline: f32,
    text: &str,
) {
    let (size, fill) = (style.size, style.fill);
    let edge = style.edge.unwrap_or(OUTLINE);
    let radius = (size / OUTLINE_DIVISOR).round().max(2.0) as i32;

    let mut marks: Vec<(i32, i32, f32)> = Vec::new();
    for_each_glyph_pixel(font, size, x, baseline, text, &mut |px, py, coverage| {
        marks.push((px, py, coverage));
    });
    let Some(&(first_x, first_y, _)) = marks.first() else {
        return;
    };

    let (mut min_x, mut min_y, mut max_x, mut max_y) = (first_x, first_y, first_x, first_y);
    for &(px, py, _) in &marks {
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    // Room for the edge to grow into.
    min_x -= radius;
    min_y -= radius;
    max_x += radius;
    max_y += radius;

    let w = (max_x - min_x + 1) as usize;
    let h = (max_y - min_y + 1) as usize;
    let mut cover = vec![0f32; w * h];
    for &(px, py, coverage) in &marks {
        let i = (py - min_y) as usize * w + (px - min_x) as usize;
        cover[i] = cover[i].max(coverage);
    }

    // Grow it. A square max-filter, done as two one-dimensional passes: the
    // same result as testing every pixel in the neighbourhood, at 2r work per
    // pixel instead of r squared. At three pixels the difference between a
    // square and a disc is not visible; the difference in time is.
    let mut wide = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let lo = x.saturating_sub(radius as usize);
            let hi = (x + radius as usize).min(w - 1);
            let mut best = 0f32;
            for sx in lo..=hi {
                best = best.max(cover[y * w + sx]);
            }
            wide[y * w + x] = best;
        }
    }
    let mut grown = vec![0f32; w * h];
    for y in 0..h {
        let lo = y.saturating_sub(radius as usize);
        let hi = (y + radius as usize).min(h - 1);
        for x in 0..w {
            let mut best = 0f32;
            for (sy, _) in (lo..=hi).enumerate() {
                best = best.max(wide[(lo + sy) * w + x]);
            }
            grown[y * w + x] = best;
        }
    }

    let mut put = |x: usize, y: usize, colour: Rgb<u8>, alpha: f32| {
        if alpha <= 0.0 {
            return;
        }
        let (px, py) = (min_x + x as i32, min_y + y as i32);
        if px < 0 || py < 0 || px >= WIDTH as i32 || py >= HEIGHT as i32 {
            return;
        }
        let under = *canvas.get_pixel(px as u32, py as u32);
        canvas.put_pixel(px as u32, py as u32, blend(under, colour, alpha));
    };

    // The edge first, then the letter over it: where the letter is solid the
    // edge is invisible anyway, and this keeps the fill's own antialiasing.
    for y in 0..h {
        for x in 0..w {
            put(x, y, edge, grown[y * w + x]);
        }
    }
    for y in 0..h {
        for x in 0..w {
            put(x, y, fill, cover[y * w + x]);
        }
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
        let png = generated(&sample()).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (WIDTH, HEIGHT));
    }

    /// The card has to have ink on it. A font that failed to load, or a
    /// layout that put the text off the canvas, would otherwise pass every
    /// other assertion here.
    #[test]
    fn a_generated_card_has_text_drawn_on_it() {
        let png = generated(&sample()).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgb8();
        let inked = image.pixels().filter(|p| **p != GREEN).count();
        assert!(
            inked > 5_000,
            "only {inked} pixels differ from the background"
        );
    }

    /// One card's figures, so a change to `Card` is one edit here.
    fn sample() -> Card<'static> {
        Card {
            name: "Pierogi Ruskie",
            servings: 4.0,
            calories_per_serving: 320.0,
            protein_per_serving: 12.0,
            carbs_per_serving: 41.0,
            fat_per_serving: 9.0,
        }
    }

    /// What a serving holds is the reason someone opens the link, so the card
    /// has to say it, in words a stranger reads rather than the app's own
    /// shorthand.
    #[test]
    fn a_card_states_the_macros_of_one_serving() {
        assert_eq!(sample().macros(), "12 g protein · 41 g carbs · 9 g fat");
        assert_eq!(sample().subtitle(), "4 servings · 320 kcal per serving");
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

        let png = from_photo(&source, &sample()).unwrap();
        let decoded = image::load_from_memory(&png).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (WIDTH, HEIGHT));
    }

    /// Over a photograph the caption carries its own contrast: each letter is
    /// drawn with a dark edge around it, which is what lets the darkening
    /// underneath stay light enough to still show the food.
    ///
    /// Tested on a near-white photo, where the gentle scrim alone could not
    /// produce a dark pixel: 235 taken down by `SCRIM_MAX` is still about
    /// 136, so anything darker than that is the outline and nothing else.
    #[test]
    fn a_caption_over_a_photo_is_outlined() {
        let flat = image::RgbImage::from_pixel(1200, 630, Rgb([235, 235, 235]));
        let mut source = Vec::new();
        image::DynamicImage::ImageRgb8(flat)
            .write_to(
                &mut std::io::Cursor::new(&mut source),
                image::ImageFormat::Png,
            )
            .unwrap();

        let image = image::load_from_memory(&from_photo(&source, &sample()).unwrap())
            .unwrap()
            .to_rgb8();

        assert!(
            image.get_pixel(5, 5).0[0] > 200,
            "the top must be untouched"
        );

        let band = |p: &(u32, u32, &Rgb<u8>)| p.1 > HEIGHT - 240;
        let dark = image
            .enumerate_pixels()
            .filter(|p| band(p) && p.2 .0[0] < 80)
            .count();
        let light = image
            .enumerate_pixels()
            .filter(|p| band(p) && p.2 .0[0] > 220)
            .count();
        assert!(
            dark > 3_000,
            "only {dark} dark pixels: the letters have no edge, so white text \
             on a pale photo would be invisible"
        );
        assert!(light > 3_000, "only {light} light pixels: no caption drawn");
    }

    /// The gentle scrim still has to be gentle: burying the photograph is the
    /// failure this outline exists to avoid.
    #[test]
    fn the_photo_is_still_visible_under_the_caption() {
        let flat = image::RgbImage::from_pixel(1200, 630, Rgb([200, 120, 60]));
        let mut source = Vec::new();
        image::DynamicImage::ImageRgb8(flat)
            .write_to(
                &mut std::io::Cursor::new(&mut source),
                image::ImageFormat::Png,
            )
            .unwrap();

        let image = image::load_from_memory(&from_photo(&source, &sample()).unwrap())
            .unwrap()
            .to_rgb8();
        // A row between the lines of type, at the very foot of the card.
        let row: u32 = HEIGHT - 8;
        let mean: f32 = (0..WIDTH)
            .map(|x| image.get_pixel(x, row).0[0] as f32)
            .sum::<f32>()
            / WIDTH as f32;
        assert!(
            mean > 90.0,
            "the foot averages {mean}, which is too dark to see the food through"
        );
    }

    #[test]
    fn a_file_that_is_not_an_image_does_not_produce_a_card() {
        assert!(from_photo(b"not a photo at all", &sample()).is_err());
    }
}
