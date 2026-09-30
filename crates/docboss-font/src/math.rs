//! The OpenType `MATH` table's constants: how a math font sizes scripts
//! and places fractions, radicals, bars and limits.

use crate::bytes::{i16_at, u16_at};

/// The `MathConstants` a layout uses, in em.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MathConstants {
    /// The size of scripts and of scripts of scripts, as a fraction.
    pub script_scale: f32,
    pub script_script_scale: f32,
    /// The height of the math axis above the baseline.
    pub axis_height: f32,
    pub subscript_shift_down: f32,
    pub superscript_shift_up: f32,
    pub upper_limit_gap: f32,
    pub lower_limit_gap: f32,
    pub numerator_shift_up: f32,
    pub numerator_display_shift_up: f32,
    pub denominator_shift_down: f32,
    pub denominator_display_shift_down: f32,
    pub numerator_gap: f32,
    pub numerator_display_gap: f32,
    pub fraction_rule: f32,
    pub denominator_gap: f32,
    pub denominator_display_gap: f32,
    pub overbar_gap: f32,
    pub overbar_rule: f32,
    pub underbar_gap: f32,
    pub radical_gap: f32,
    pub radical_display_gap: f32,
    pub radical_rule: f32,
    pub radical_extra_ascender: f32,
}

impl Default for MathConstants {
    /// Values close to those of common math fonts, for faces without a
    /// `MATH` table.
    fn default() -> Self {
        MathConstants {
            script_scale: 0.7,
            script_script_scale: 0.5,
            axis_height: 0.25,
            subscript_shift_down: 0.2,
            superscript_shift_up: 0.4,
            upper_limit_gap: 0.1,
            lower_limit_gap: 0.1,
            numerator_shift_up: 0.4,
            numerator_display_shift_up: 0.68,
            denominator_shift_down: 0.35,
            denominator_display_shift_down: 0.68,
            numerator_gap: 0.04,
            numerator_display_gap: 0.12,
            fraction_rule: 0.066,
            denominator_gap: 0.04,
            denominator_display_gap: 0.12,
            overbar_gap: 0.15,
            overbar_rule: 0.066,
            underbar_gap: 0.15,
            radical_gap: 0.06,
            radical_display_gap: 0.15,
            radical_rule: 0.066,
            radical_extra_ascender: 0.066,
        }
    }
}

impl MathConstants {
    /// Reads the constants of a `MATH` table: its header's
    /// `mathConstantsOffset`, two percentages, two heights, then value
    /// records of a value and a device offset each, in font units.
    pub(crate) fn parse(table: &[u8], units_per_em: u16) -> Option<MathConstants> {
        let start = usize::from(u16_at(table, 4)?);
        if start == 0 {
            return None;
        }
        let em = f32::from(units_per_em.max(16));
        let percent =
            |at: usize| i16_at(table, start + at).map(|v| f32::from(v.clamp(1, 100)) / 100.0);
        let value =
            |record: usize| i16_at(table, start + 8 + record * 4).map(|v| f32::from(v) / em);
        Some(MathConstants {
            script_scale: percent(0)?,
            script_script_scale: percent(2)?,
            axis_height: value(1)?,
            subscript_shift_down: value(4)?,
            superscript_shift_up: value(7)?,
            upper_limit_gap: value(14)?,
            lower_limit_gap: value(16)?,
            numerator_shift_up: value(28)?,
            numerator_display_shift_up: value(29)?,
            denominator_shift_down: value(30)?,
            denominator_display_shift_down: value(31)?,
            numerator_gap: value(32)?,
            numerator_display_gap: value(33)?,
            fraction_rule: value(34)?,
            denominator_gap: value(35)?,
            denominator_display_gap: value(36)?,
            overbar_gap: value(39)?,
            overbar_rule: value(40)?,
            underbar_gap: value(42)?,
            radical_gap: value(45)?,
            radical_display_gap: value(46)?,
            radical_rule: value(47)?,
            radical_extra_ascender: value(48)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `MATH` table's constants come from their record positions, in em.
    #[test]
    fn math_constants_by_position() {
        let mut table = vec![0u8; 10];
        table[4..6].copy_from_slice(&10u16.to_be_bytes());
        table.extend_from_slice(&70i16.to_be_bytes());
        table.extend_from_slice(&50i16.to_be_bytes());
        table.extend_from_slice(&[0; 4]);
        for record in 0..56i16 {
            table.extend_from_slice(&(record * 10).to_be_bytes());
            table.extend_from_slice(&[0, 0]);
        }
        let c = MathConstants::parse(&table, 1000).unwrap();
        assert_eq!((c.script_scale, c.script_script_scale), (0.7, 0.5));
        assert_eq!(c.axis_height, 0.01);
        assert_eq!(c.fraction_rule, 0.34);
        assert_eq!(c.radical_extra_ascender, 0.48);
        assert!(MathConstants::parse(&[0; 8], 1000).is_none());
    }
}
