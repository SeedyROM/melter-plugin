#![allow(dead_code)]

use std::f32::consts::PI;

#[inline(always)]
pub fn bridge_rectifier(input: f32) -> f32 {
    input.abs().min(PI).sin()
}

#[inline(always)]
pub fn cubic(x: f32, drive: f32, offset: f32) -> f32 {
    #[inline(always)]
    fn clip(lo: f32, hi: f32, x: f32) -> f32 {
        x.max(lo).min(hi)
    }

    #[inline(always)]
    fn cubic(x: f32) -> f32 {
        x - x * x * x / 3.0
    }

    // Calculate pregain
    let pregain = 10.0f32.powf(2.0 * drive);

    // Apply pregain, add offset, clip, apply cubic nonlinearity
    let result = x * pregain;
    let result = result + offset;
    let result = clip(-1.0, 1.0, result);
    let result = cubic(result);

    // Calculate and apply postgain
    let postgain = 1.0f32.max(1.0 / pregain);
    result * postgain
}
