//! Page preview: a rendered page painted with `▀` half-blocks, the upper
//! pixel of each cell as the foreground color and the lower pixel as the
//! background, two vertical pixels per terminal cell.

use docboss_render::Pixmap;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// The scale, in pixels per point, that fits a `width` by `height` point
/// page into `cols` by `rows` cells.
pub fn fit_scale(width: f32, height: f32, cols: u16, rows: u16) -> f32 {
    if !(width > 0.0 && height > 0.0) || cols == 0 || rows == 0 {
        return 0.1;
    }
    let horizontal = f32::from(cols) / width;
    let vertical = f32::from(rows) * 2.0 / height;
    horizontal.min(vertical).max(0.01)
}

fn over_white(pixel: [u8; 4]) -> Color {
    let alpha = u16::from(pixel[3]);
    let blend = |c: u8| ((u16::from(c) * alpha + 255 * (255 - alpha)) / 255) as u8;
    Color::Rgb(blend(pixel[0]), blend(pixel[1]), blend(pixel[2]))
}

/// The pixmap as half-block lines, centered horizontally in `cols`.
pub fn half_blocks(pixmap: &Pixmap, cols: u16, rows: u16) -> Vec<Line<'static>> {
    let width = pixmap.width.min(u32::from(cols));
    let pad = (u32::from(cols) - width) / 2;
    let cell_rows = pixmap.height.div_ceil(2).min(u32::from(rows));
    (0..cell_rows)
        .map(|row| {
            let mut spans = vec![Span::raw(" ".repeat(pad as usize))];
            spans.extend((0..width).map(|x| {
                let top = pixmap.pixel(x, row * 2).map_or(Color::Reset, over_white);
                let bottom = pixmap
                    .pixel(x, row * 2 + 1)
                    .map_or(Color::Reset, over_white);
                Span::styled("\u{2580}", Style::default().fg(top).bg(bottom))
            }));
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_fits_the_tighter_dimension() {
        let scale = fit_scale(612.0, 792.0, 80, 40);
        assert!(612.0 * scale <= 80.0 + 0.01);
        assert!(792.0 * scale <= 80.0 + 0.01);
        assert_eq!(fit_scale(0.0, 792.0, 80, 40), 0.1);
    }

    #[test]
    fn half_blocks_take_two_pixel_rows_per_line() {
        let mut pixmap = Pixmap::new(4, 5).unwrap();
        pixmap.data[0..4].copy_from_slice(&[0, 0, 0, 255]);
        let lines = half_blocks(&pixmap, 8, 10);
        assert_eq!(lines.len(), 3);
        let first = &lines[0].spans[1];
        assert_eq!(first.style.fg, Some(Color::Rgb(0, 0, 0)));
        assert_eq!(first.style.bg, Some(Color::Rgb(255, 255, 255)));
        assert_eq!(lines[0].spans[0].content, "  ");
    }
}
