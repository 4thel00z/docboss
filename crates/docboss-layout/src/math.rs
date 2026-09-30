//! Office Math laid out as boxes (ECMA-376 Part 1 §22.1): runs, fractions,
//! scripts, radicals, n-ary operators, delimiters, matrices, accents,
//! bars, functions, group characters, limits, arrays and border boxes,
//! placed by the math font's `MATH` constants, or by common values when
//! the face has none.

use docboss_font::{bounds, MathConstants, Seg};
use docboss_model::{Color, FractionKind, LineCap, LineJoin, Math, MathNode, MathStyle};

use crate::flow::Ctx;
use crate::label;
use crate::shape::RunStyle;
use crate::{Item, LineStyle, Rect, Stroke};

/// The faces math is set in, the first with a `MATH` table that is
/// installed; Cambria Math otherwise, through its text substitute.
const MATH_FONTS: [&str; 4] = [
    "Cambria Math",
    "STIX Two Math",
    "Latin Modern Math",
    "STIX Math",
];
/// The deepest nesting laid out.
const MAX_DEPTH: usize = 48;

/// A laid-out piece of math: its items relative to its left end on the
/// baseline (y down), its width, and how far it reaches above and below
/// the baseline, in points.
#[derive(Debug, Clone, Default)]
pub(crate) struct MathBox {
    pub items: Vec<Item>,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

impl MathBox {
    fn place(&mut self, other: MathBox, dx: f32, dy: f32) {
        self.items.extend(other.items.into_iter().map(|mut item| {
            item.offset(dx, dy);
            item
        }));
        self.ascent = self.ascent.max(other.ascent - dy);
        self.descent = self.descent.max(other.descent + dy);
        self.width = self.width.max(dx + other.width);
    }

    fn append(&mut self, other: MathBox) {
        let x = self.width;
        self.place(other, x, 0.0);
    }

    fn space(&mut self, width: f32) {
        self.width += width;
    }
}

/// The size and style math at one depth is drawn in.
#[derive(Clone)]
struct Env<'s> {
    size: f32,
    display: bool,
    level: u8,
    color: Color,
    base: &'s RunStyle,
    constants: MathConstants,
    font: &'static str,
}

impl<'s> Env<'s> {
    fn script(&self) -> Env<'s> {
        let scale = match self.level {
            0 => self.constants.script_scale,
            1 => self.constants.script_script_scale / self.constants.script_scale.max(0.1),
            _ => 1.0,
        };
        Env {
            size: (self.size * scale).max(1.0),
            display: false,
            level: self.level.saturating_add(1).min(2),
            ..self.clone()
        }
    }

    fn text(&self) -> Env<'s> {
        Env {
            display: false,
            ..self.clone()
        }
    }

    fn em(&self, value: f32) -> f32 {
        value * self.size
    }

    fn style(&self, style: MathStyle) -> RunStyle {
        if style == MathStyle::Text {
            let mut own = self.base.clone();
            own.size = self.size;
            own.base_size = self.size;
            own.rise = 0.0;
            return own;
        }
        let bold = matches!(style, MathStyle::Bold | MathStyle::BoldItalic);
        label::style(Some(self.font), self.size, bold, false, self.color)
    }

    fn rule(&self, value: f32) -> f32 {
        self.em(value).max(0.4)
    }
}

/// The math alphanumeric form of a letter in `style`.
fn styled(c: char, style: MathStyle) -> char {
    let offset = |base: u32, from: char| char::from_u32(base + (c as u32 - from as u32));
    let mapped = match (style, c) {
        (MathStyle::Math, 'h') => Some('\u{210E}'),
        (MathStyle::Math, 'a'..='z') => offset(0x1D44E, 'a'),
        (MathStyle::Math, 'A'..='Z') => offset(0x1D434, 'A'),
        (MathStyle::Math, 'α'..='ω') => offset(0x1D6FC, 'α'),
        (MathStyle::Bold, 'a'..='z') => offset(0x1D41A, 'a'),
        (MathStyle::Bold, 'A'..='Z') => offset(0x1D400, 'A'),
        (MathStyle::BoldItalic, 'a'..='z') => offset(0x1D482, 'a'),
        (MathStyle::BoldItalic, 'A'..='Z') => offset(0x1D468, 'A'),
        (_, '-') if style != MathStyle::Text => Some('−'),
        _ => None,
    };
    mapped.unwrap_or(c)
}

/// Whether a character is spaced as a binary operator or a relation.
fn spacing(c: char) -> Option<f32> {
    match c {
        '+' | '−' | '±' | '∓' | '×' | '÷' | '⋅' | '∙' | '∗' | '∘' | '∪' | '∩' | '⊕' | '⊗' => {
            Some(4.0 / 18.0)
        }
        '=' | '<' | '>' | '≤' | '≥' | '≠' | '≈' | '≡' | '∼' | '≅' | '∝' | '→' | '←' | '↔' | '⇒'
        | '⇐' | '⇔' | '∈' | '∉' | '⊂' | '⊃' | '⊆' | '⊇' | '≪' | '≫' | '∥' | '⊥' => {
            Some(5.0 / 18.0)
        }
        _ => None,
    }
}

/// The ink of a glyph run: how far it reaches above and below its
/// baseline.
fn ink(ctx: &mut Ctx<'_>, text: &str, style: &RunStyle) -> (f32, f32) {
    let (mut top, mut bottom) = (0.0f32, 0.0f32);
    for c in text.chars() {
        let glyph = ctx.shaper.glyph(c, style);
        let Some(font) = ctx.shaper.font(glyph.font) else {
            continue;
        };
        let k = glyph.size / f32::from(font.units_per_em().max(16));
        if let Some([_, y0, _, y1]) = bounds(&font.outline(glyph.id)) {
            top = top.max(y1 * k);
            bottom = bottom.max(-y0 * k);
        }
    }
    (top, bottom)
}

fn glyphs(ctx: &mut Ctx<'_>, text: &str, style: &RunStyle) -> MathBox {
    let mut items = Vec::new();
    let width = label::draw(ctx, &mut items, text, style, 0.0, 0.0);
    let (ascent, descent) = ink(ctx, text, style);
    MathBox {
        items,
        width,
        ascent,
        descent,
    }
}

fn rule_rect(x: f32, y: f32, width: f32, height: f32, color: Color) -> Item {
    Item::Rect {
        rect: Rect::new(x, y, width.max(0.0), height.max(0.0)),
        color,
    }
}

/// Lays out a math zone in `base`'s size and color.
pub(crate) fn layout_math(ctx: &mut Ctx<'_>, math: &Math, base: &RunStyle) -> MathBox {
    let found = MATH_FONTS.iter().find_map(|name| {
        let probe = label::style(Some(name), base.size, false, false, base.color);
        let font = ctx
            .shaper
            .primary(&probe)
            .and_then(|id| ctx.shaper.font(id))?;
        let named = font.names().family.to_ascii_lowercase().replace(' ', "");
        let math = font.math_constants()?;
        (named == name.to_ascii_lowercase().replace(' ', "")).then_some((*name, math))
    });
    let (font, constants) = found.unwrap_or((MATH_FONTS[0], MathConstants::default()));
    let env = Env {
        size: base.base_size.max(1.0),
        display: math.display,
        level: 0,
        color: base.color,
        base,
        constants,
        font,
    };
    list(ctx, &math.nodes, &env, 0)
}

fn list(ctx: &mut Ctx<'_>, nodes: &[MathNode], env: &Env<'_>, depth: usize) -> MathBox {
    let mut out = MathBox::default();
    if depth >= MAX_DEPTH {
        return out;
    }
    let mut operand = false;
    for node in nodes {
        match node {
            MathNode::Run { text, style } => {
                run(ctx, text, *style, env, &mut out, &mut operand);
                continue;
            }
            other => out.append(node_box(ctx, other, env, depth + 1)),
        }
        operand = true;
    }
    out
}

fn run(
    ctx: &mut Ctx<'_>,
    text: &str,
    style: MathStyle,
    env: &Env<'_>,
    out: &mut MathBox,
    operand: &mut bool,
) {
    let drawn = env.style(style);
    let mut pending = String::new();
    let flush = |ctx: &mut Ctx<'_>, pending: &mut String, out: &mut MathBox| {
        if pending.is_empty() {
            return;
        }
        out.append(glyphs(ctx, pending, &drawn));
        pending.clear();
    };
    for c in text.chars() {
        let c = styled(c, style);
        let gap = spacing(c).filter(|_| env.level == 0 && style != MathStyle::Text);
        let Some(gap) = gap else {
            pending.push(c);
            *operand = !c.is_whitespace();
            continue;
        };
        flush(ctx, &mut pending, out);
        let binary = gap < 0.25;
        let spaced = !binary || *operand;
        if spaced {
            out.space(env.em(gap));
        }
        out.append(glyphs(ctx, &c.to_string(), &drawn));
        if spaced {
            out.space(env.em(gap));
        }
        *operand = false;
    }
    flush(ctx, &mut pending, out);
}

fn node_box(ctx: &mut Ctx<'_>, node: &MathNode, env: &Env<'_>, depth: usize) -> MathBox {
    match node {
        MathNode::Run { text, style } => {
            let mut out = MathBox::default();
            run(ctx, text, *style, env, &mut out, &mut true);
            out
        }
        MathNode::Fraction {
            kind,
            numerator,
            denominator,
        } => fraction(ctx, *kind, numerator, denominator, env, depth),
        MathNode::Script {
            base,
            sub,
            sup,
            pre,
        } => {
            let base = list(ctx, base, env, depth);
            scripts(ctx, base, sub.as_deref(), sup.as_deref(), *pre, env, depth)
        }
        MathNode::Radical { degree, body } => radical(ctx, degree.as_deref(), body, env, depth),
        MathNode::Nary {
            operator,
            sub,
            sup,
            limits_under,
            body,
        } => nary(
            ctx,
            *operator,
            sub.as_deref(),
            sup.as_deref(),
            *limits_under,
            body,
            env,
            depth,
        ),
        MathNode::Delimiter {
            open,
            close,
            separator,
            items,
        } => delimited(ctx, *open, *close, *separator, items, env, depth),
        MathNode::Matrix { rows } => {
            let cells: Vec<Vec<MathBox>> = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| list(ctx, cell, &env.text(), depth))
                        .collect()
                })
                .collect();
            grid(cells, env, env.em(1.0))
        }
        MathNode::Array { rows } => {
            let cells: Vec<Vec<MathBox>> = rows
                .iter()
                .map(|row| vec![list(ctx, row, env, depth)])
                .collect();
            grid(cells, env, 0.0)
        }
        MathNode::Accent { accent, body } => accented(ctx, *accent, body, env, depth),
        MathNode::Bar { top, body } => {
            let mut body = list(ctx, body, env, depth);
            let c = env.constants;
            let rule = env.rule(c.overbar_rule);
            let gap = env.em(c.overbar_gap.max(0.05));
            let width = body.width;
            let y = match top {
                true => -(body.ascent + gap + rule),
                false => body.descent + env.em(c.underbar_gap.max(0.05)),
            };
            body.items.push(rule_rect(0.0, y, width, rule, env.color));
            match top {
                true => body.ascent = -y + rule * 0.5,
                false => body.descent = y + rule * 1.5,
            }
            body
        }
        MathNode::Function { name, body } => {
            let mut out = list(ctx, name, env, depth);
            out.space(env.em(3.0 / 18.0));
            out.append(list(ctx, body, env, depth));
            out
        }
        MathNode::GroupCharacter {
            character,
            top,
            body,
        } => group_character(ctx, *character, *top, body, env, depth),
        MathNode::Limit { base, limit, lower } => {
            let base = list(ctx, base, env, depth);
            let limit = list(ctx, limit, &env.script(), depth);
            stack_limits(
                base,
                (!lower).then_some(limit.clone()),
                lower.then_some(limit),
                env,
            )
        }
        MathNode::BorderBox { body } => {
            let body = list(ctx, body, env, depth);
            let pad = env.em(0.1);
            let mut out = MathBox {
                width: body.width + 2.0 * pad,
                ascent: body.ascent + pad,
                descent: body.descent + pad,
                items: Vec::new(),
            };
            let (w, a, d) = (out.width, out.ascent, out.descent);
            out.place(body, pad, 0.0);
            out.items.push(Item::Outline {
                rect: Rect::new(0.0, -a, w, a + d),
                width: env.rule(0.05),
                color: env.color,
                style: LineStyle::Solid,
                cap: LineCap::Flat,
                join: LineJoin::Miter,
            });
            out
        }
    }
}

/// A fraction (ECMA-376 Part 1 §22.1.2.36): numerator over denominator
/// about the math axis with a rule between, without one for `noBar`, and
/// on one line for the linear and skewed kinds. Inline fractions keep the
/// text size and the display gaps, as Word sets them unless
/// `m:smallFrac` asks otherwise.
fn fraction(
    ctx: &mut Ctx<'_>,
    kind: FractionKind,
    numerator: &[MathNode],
    denominator: &[MathNode],
    env: &Env<'_>,
    depth: usize,
) -> MathBox {
    if matches!(kind, FractionKind::Linear | FractionKind::Skewed) {
        let mut out = list(ctx, numerator, env, depth);
        let slash = if kind == FractionKind::Skewed {
            "⁄"
        } else {
            "/"
        };
        out.append(glyphs(ctx, slash, &env.style(MathStyle::Plain)));
        out.append(list(ctx, denominator, env, depth));
        return out;
    }
    let inner = match env.level {
        0 => env.text(),
        _ => env.clone(),
    };
    let top = list(ctx, numerator, &inner, depth);
    let bottom = list(ctx, denominator, &inner, depth);
    let c = env.constants;
    let axis = env.em(c.axis_height);
    let rule = match kind {
        FractionKind::NoBar => 0.0,
        _ => env.rule(c.fraction_rule),
    };
    let (shift_up, shift_down, gap_up, gap_down) = match env.display {
        true => (
            c.numerator_display_shift_up,
            c.denominator_display_shift_down,
            c.numerator_display_gap,
            c.denominator_display_gap,
        ),
        false => (
            c.numerator_shift_up,
            c.denominator_shift_down,
            c.numerator_display_gap,
            c.denominator_display_gap,
        ),
    };
    let up = env
        .em(shift_up)
        .max(axis + rule / 2.0 + env.em(gap_up) + top.descent);
    let down = env
        .em(shift_down)
        .max(bottom.ascent + env.em(gap_down) + rule / 2.0 - axis);
    let pad = env.em(0.08);
    let width = top.width.max(bottom.width) + 2.0 * pad;
    let mut out = MathBox {
        width,
        ..MathBox::default()
    };
    let (tw, bw) = (top.width, bottom.width);
    out.place(top, (width - tw) / 2.0, -up);
    out.place(bottom, (width - bw) / 2.0, down);
    if rule > 0.0 {
        out.items.push(rule_rect(
            pad / 2.0,
            -axis - rule / 2.0,
            width - pad,
            rule,
            env.color,
        ));
    }
    let mut spaced = MathBox::default();
    spaced.space(env.em(0.05));
    spaced.append(out);
    spaced.space(env.em(0.05));
    spaced
}

/// Sub- and superscripts (§22.1.2.101, §22.1.2.105, §22.1.2.103) after
/// the base, or before it (§22.1.2.99), at script size.
fn scripts(
    ctx: &mut Ctx<'_>,
    base: MathBox,
    sub: Option<&[MathNode]>,
    sup: Option<&[MathNode]>,
    pre: bool,
    env: &Env<'_>,
    depth: usize,
) -> MathBox {
    let small = env.script();
    let sub = sub.map(|s| list(ctx, s, &small, depth));
    let sup = sup.map(|s| list(ctx, s, &small, depth));
    let c = env.constants;
    let mut up = env
        .em(c.superscript_shift_up)
        .max(base.ascent - env.em(0.35));
    let mut down = env
        .em(c.subscript_shift_down)
        .max(base.descent - env.em(0.1));
    if let (Some(sub), Some(sup)) = (&sub, &sup) {
        let gap = (up - sup.descent) - (sub.ascent - down);
        let need = env.em(0.2);
        if gap < need {
            down += (need - gap) / 2.0;
            up += (need - gap) / 2.0;
        }
    }
    if sub.is_some() && sup.is_none() {
        down = down.max(env.em(0.15));
    }
    let scripts_width = sub
        .as_ref()
        .map_or(0.0, |b| b.width)
        .max(sup.as_ref().map_or(0.0, |b| b.width));
    let after = env.em(0.05);
    let mut out = MathBox::default();
    let base_x = if pre { scripts_width + after } else { 0.0 };
    let script_x = if pre { 0.0 } else { base.width };
    let base_width = base.width;
    out.place(base, base_x, 0.0);
    if let Some(sup) = sup {
        let x = if pre {
            scripts_width - sup.width
        } else {
            script_x
        };
        out.place(sup, x, -up);
    }
    if let Some(sub) = sub {
        let x = if pre {
            scripts_width - sub.width
        } else {
            script_x
        };
        out.place(sub, x, down);
    }
    out.width = base_width + scripts_width + after;
    out
}

/// Limits centered over and under a base (§22.1.2.54, §22.1.2.56).
fn stack_limits(
    base: MathBox,
    over: Option<MathBox>,
    under: Option<MathBox>,
    env: &Env<'_>,
) -> MathBox {
    let c = env.constants;
    let width = base
        .width
        .max(over.as_ref().map_or(0.0, |b| b.width))
        .max(under.as_ref().map_or(0.0, |b| b.width));
    let mut out = MathBox {
        width,
        ..MathBox::default()
    };
    let (ascent, descent, base_width) = (base.ascent, base.descent, base.width);
    out.place(base, (width - base_width) / 2.0, 0.0);
    if let Some(over) = over {
        let dy = -(ascent + env.em(c.upper_limit_gap.max(0.05)) + over.descent);
        let w = over.width;
        out.place(over, (width - w) / 2.0, dy);
    }
    if let Some(under) = under {
        let dy = descent + env.em(c.lower_limit_gap.max(0.05)) + under.ascent;
        let w = under.width;
        out.place(under, (width - w) / 2.0, dy);
    }
    out
}

/// The items of `text` stretched by `scale` vertically about the math
/// axis to span `half` above and below it.
fn stretched_vertically(
    ctx: &mut Ctx<'_>,
    text: &str,
    style: &RunStyle,
    axis: f32,
    half: f32,
) -> MathBox {
    let natural = glyphs(ctx, text, style);
    let (top, bottom) = ink(ctx, text, style);
    let height = top + bottom;
    if height <= 0.0 || half * 2.0 <= height * 1.05 {
        return natural;
    }
    let scale = half * 2.0 / height;
    let ty = (-axis - half) + scale * top;
    let mut items = vec![Item::TransformBegin([1.0, 0.0, 0.0, scale, 0.0, ty])];
    items.extend(natural.items);
    items.push(Item::TransformEnd);
    MathBox {
        items,
        width: natural.width,
        ascent: axis + half,
        descent: (half - axis).max(0.0),
    }
}

/// An n-ary operator (§22.1.2.70), enlarged in display math and centered
/// on the axis, its limits under and over it or as scripts, then its
/// operand.
#[allow(clippy::too_many_arguments)]
fn nary(
    ctx: &mut Ctx<'_>,
    operator: char,
    sub: Option<&[MathNode]>,
    sup: Option<&[MathNode]>,
    limits_under: bool,
    body: &[MathNode],
    env: &Env<'_>,
    depth: usize,
) -> MathBox {
    let integral = matches!(operator, '∫' | '∬' | '∭' | '∮' | '∯' | '∰');
    let grow = match (env.display, integral) {
        (true, true) => 2.0,
        (true, false) => 1.45,
        (false, true) => 1.3,
        (false, false) => 1.1,
    };
    let op_env = Env {
        size: env.size * grow,
        ..env.clone()
    };
    let style = op_env.style(MathStyle::Plain);
    let text = operator.to_string();
    let mut op = glyphs(ctx, &text, &style);
    let (top, bottom) = ink(ctx, &text, &style);
    let axis = env.em(env.constants.axis_height);
    let shift = (top - bottom) / 2.0 - axis;
    for item in &mut op.items {
        item.offset(0.0, shift);
    }
    op.ascent = top - shift;
    op.descent = bottom + shift;
    let mut out = match limits_under {
        true => {
            let small = env.script();
            let over = sup.map(|s| list(ctx, s, &small, depth));
            let under = sub.map(|s| list(ctx, s, &small, depth));
            stack_limits(op, over, under, env)
        }
        false => {
            let small = env.script();
            let sup_box = sup.map(|s| list(ctx, s, &small, depth));
            let sub_box = sub.map(|s| list(ctx, s, &small, depth));
            let mut out = MathBox::default();
            let (op_width, op_ascent, op_descent) = (op.width, op.ascent, op.descent);
            out.place(op, 0.0, 0.0);
            let slant = if integral { env.em(0.15) * grow } else { 0.0 };
            if let Some(sup) = sup_box {
                let dy = -(op_ascent - sup.ascent * 0.6);
                out.place(sup, op_width, dy);
            }
            if let Some(sub) = sub_box {
                let dy = op_descent - sub.descent.max(sub.ascent * 0.3);
                out.place(sub, (op_width - slant).max(0.0), dy);
            }
            out
        }
    };
    out.space(env.em(3.0 / 18.0));
    out.append(list(ctx, body, env, depth));
    out
}

/// Delimiters around items (§22.1.2.24), grown to the height of what
/// they hold about the axis, the items separated by the separator.
fn delimited(
    ctx: &mut Ctx<'_>,
    open: Option<char>,
    close: Option<char>,
    separator: char,
    items: &[Vec<MathNode>],
    env: &Env<'_>,
    depth: usize,
) -> MathBox {
    let style = env.style(MathStyle::Plain);
    let mut inner = MathBox::default();
    let boxes: Vec<MathBox> = items.iter().map(|i| list(ctx, i, env, depth)).collect();
    let axis = env.em(env.constants.axis_height);
    let half = boxes
        .iter()
        .map(|b| (b.ascent - axis).max(b.descent + axis))
        .fold(env.em(0.5), f32::max)
        * 1.08;
    let count = boxes.len();
    for (i, b) in boxes.into_iter().enumerate() {
        inner.append(b);
        if i + 1 < count {
            inner.append(stretched_vertically(
                ctx,
                &separator.to_string(),
                &style,
                axis,
                half,
            ));
        }
    }
    let mut out = MathBox::default();
    if let Some(open) = open {
        out.append(stretched_vertically(
            ctx,
            &open.to_string(),
            &style,
            axis,
            half,
        ));
    }
    out.append(inner);
    if let Some(close) = close {
        out.append(stretched_vertically(
            ctx,
            &close.to_string(),
            &style,
            axis,
            half,
        ));
    }
    out
}

/// Cells in rows and columns (§22.1.2.60, §22.1.2.34), each centered in
/// its column, the whole centered on the axis.
fn grid(rows: Vec<Vec<MathBox>>, env: &Env<'_>, column_gap: f32) -> MathBox {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0.0f32; columns];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.width);
        }
    }
    let heights: Vec<(f32, f32)> = rows
        .iter()
        .map(|row| {
            row.iter().fold((env.em(0.7), env.em(0.25)), |(a, d), c| {
                (a.max(c.ascent), d.max(c.descent))
            })
        })
        .collect();
    let row_gap = env.em(0.25);
    let total: f32 = heights.iter().map(|(a, d)| a + d).sum::<f32>()
        + row_gap * rows.len().saturating_sub(1) as f32;
    let axis = env.em(env.constants.axis_height);
    let mut y = -axis - total / 2.0;
    let mut out = MathBox::default();
    for (row, (ascent, descent)) in rows.into_iter().zip(heights) {
        let baseline = y + ascent;
        let mut x = 0.0;
        for (i, cell) in row.into_iter().enumerate() {
            let w = cell.width;
            out.place(cell, x + (widths[i] - w) / 2.0, baseline);
            x += widths[i] + column_gap;
        }
        y = baseline + descent + row_gap;
    }
    out.width = widths.iter().sum::<f32>() + column_gap * columns.saturating_sub(1) as f32;
    out.ascent = out.ascent.max(axis + total / 2.0);
    out.descent = out.descent.max(total / 2.0 - axis);
    let mut spaced = MathBox::default();
    spaced.space(env.em(0.1));
    spaced.append(out);
    spaced.space(env.em(0.1));
    spaced
}

/// The spacing form of a combining accent.
fn spacing_accent(c: char) -> char {
    match c {
        '\u{0300}' => '`',
        '\u{0301}' => '´',
        '\u{0302}' => 'ˆ',
        '\u{0303}' => '˜',
        '\u{0304}' | '\u{0305}' => '¯',
        '\u{0306}' => '˘',
        '\u{0307}' => '˙',
        '\u{0308}' => '¨',
        '\u{030C}' => 'ˇ',
        '\u{20D7}' => '→',
        '\u{20D6}' => '←',
        '\u{20E1}' => '↔',
        other => other,
    }
}

/// An accent (§22.1.2.1) centered over its body; bars and arrows stretch
/// across a body wider than they are.
fn accented(
    ctx: &mut Ctx<'_>,
    accent: char,
    body: &[MathNode],
    env: &Env<'_>,
    depth: usize,
) -> MathBox {
    let body = list(ctx, body, env, depth);
    let shown = spacing_accent(accent);
    let style = env.style(MathStyle::Plain);
    let text = shown.to_string();
    let mark = glyphs(ctx, &text, &style);
    let (top, bottom) = ink(ctx, &text, &style);
    let gap = env.em(0.06);
    let dy = -(body.ascent + gap + bottom);
    let stretch = matches!(shown, '¯' | '→' | '←' | '↔') && body.width > mark.width;
    let (ascent, width) = (body.ascent, body.width);
    let mut out = body;
    match stretch {
        true => {
            let sx = width / mark.width.max(0.1);
            let mut items = vec![Item::TransformBegin([sx, 0.0, 0.0, 1.0, 0.0, dy])];
            items.extend(mark.items);
            items.push(Item::TransformEnd);
            out.items.extend(items);
        }
        false => {
            let mw = mark.width;
            out.place(mark, (width - mw) / 2.0, dy);
        }
    }
    out.ascent = out.ascent.max(ascent + gap + bottom + top);
    out
}

/// A group character (§22.1.2.41) stretched across its body, under it by
/// default.
fn group_character(
    ctx: &mut Ctx<'_>,
    character: char,
    top: bool,
    body: &[MathNode],
    env: &Env<'_>,
    depth: usize,
) -> MathBox {
    let body = list(ctx, body, env, depth);
    let style = env.style(MathStyle::Plain);
    let text = character.to_string();
    let mark = glyphs(ctx, &text, &style);
    let (ink_top, ink_bottom) = ink(ctx, &text, &style);
    let gap = env.em(0.08);
    let sx = (body.width / mark.width.max(0.1)).max(1.0);
    let dy = match top {
        true => -(body.ascent + gap + ink_bottom),
        false => body.descent + gap + ink_top,
    };
    let (ascent, descent) = (body.ascent, body.descent);
    let mut out = body;
    let mut items = vec![Item::TransformBegin([sx, 0.0, 0.0, 1.0, 0.0, dy])];
    items.extend(mark.items);
    items.push(Item::TransformEnd);
    out.items.extend(items);
    match top {
        true => out.ascent = ascent + gap + ink_top + ink_bottom,
        false => out.descent = descent + gap + ink_top + ink_bottom,
    }
    out
}

/// A radical (§22.1.2.88): a surd as tall as its body plus a gap, a rule
/// over the body, and the degree small in the surd's crook.
fn radical(
    ctx: &mut Ctx<'_>,
    degree: Option<&[MathNode]>,
    body: &[MathNode],
    env: &Env<'_>,
    depth: usize,
) -> MathBox {
    let body = list(ctx, body, env, depth);
    let c = env.constants;
    let gap = env.em(match env.display {
        true => c.radical_display_gap,
        false => c.radical_gap,
    });
    let rule = env.rule(c.radical_rule);
    let top = -(body.ascent.max(env.em(0.65)) + gap + rule / 2.0);
    let bottom = body.descent.max(env.em(0.1));
    let height = bottom - top;
    let surd = env.em(0.55).min(height * 0.6).max(env.em(0.4));
    let degree = degree.map(|d| {
        let tiny = env.script().script();
        list(ctx, d, &tiny, depth)
    });
    let lead = degree
        .as_ref()
        .map_or(0.0, |d| (d.width - surd * 0.5).max(0.0));
    let x = |v: f32| lead + v;
    let segs = vec![
        Seg::Move(x(surd * 0.05), top + height * 0.62),
        Seg::Line(x(surd * 0.28), top + height * 0.52),
        Seg::Line(x(surd * 0.55), bottom),
        Seg::Line(x(surd), top),
        Seg::Line(x(surd) + body.width + env.em(0.08), top),
    ];
    let mut out = MathBox::default();
    out.items.push(Item::Path {
        segs,
        fill: None,
        stroke: Some(Stroke {
            width: rule,
            color: env.color,
            style: LineStyle::Solid,
            cap: LineCap::Flat,
            join: LineJoin::Miter,
        }),
    });
    let body_width = body.width;
    out.place(body, x(surd + env.em(0.04)), 0.0);
    if let Some(degree) = degree {
        let dy = top + height * 0.5 - degree.descent;
        out.place(degree, 0.0, dy);
    }
    out.width = x(surd) + body_width + env.em(0.12);
    out.ascent = out
        .ascent
        .max(-top + rule / 2.0 + env.em(c.radical_extra_ascender));
    out.descent = out.descent.max(bottom);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Letters take their math italic forms and the minus sign replaces
    /// the hyphen; plain text keeps its letters.
    #[test]
    fn math_letters_are_italic() {
        assert_eq!(styled('x', MathStyle::Math), '𝑥');
        assert_eq!(styled('h', MathStyle::Math), 'ℎ');
        assert_eq!(styled('B', MathStyle::Bold), '𝐁');
        assert_eq!(styled('α', MathStyle::Math), '𝛼');
        assert_eq!(styled('x', MathStyle::Plain), 'x');
        assert_eq!(styled('-', MathStyle::Plain), '−');
        assert_eq!(styled('-', MathStyle::Text), '-');
        assert_eq!(spacing('='), Some(5.0 / 18.0));
    }
}
