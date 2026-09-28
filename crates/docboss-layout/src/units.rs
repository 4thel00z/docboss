/// Twips (1/20 point) to points.
pub fn twips_to_pt(twips: i32) -> f32 {
    twips as f32 / 20.0
}

/// EMU (1/914400 inch) to points.
pub fn emu_to_pt(emu: i64) -> f32 {
    emu as f32 / 12700.0
}
