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
    fn c3(x: f32) -> f32 {
        x - x * x * x / 3.0
    }

    // Calculate pregain
    let pregain = 10.0f32.powf(2.0 * drive);

    // Apply pregain, add offset, clip, apply cubic nonlinearity
    let result = x * pregain;
    let result = result + offset;
    let result = clip(-1.0, 1.0, result);
    let result = c3(result);

    // Calculate and apply postgain
    let postgain = 1.0f32.max(1.0 / pregain);
    result * postgain
}

pub struct SlewDistortion {
    pos_rate: f32,
    neg_rate: f32,
    last_sample: f32,
}

impl SlewDistortion {
    pub fn new(pos_rate: f32, neg_rate: f32) -> Self {
        SlewDistortion {
            pos_rate: pos_rate.max(0.0),
            neg_rate: neg_rate.max(0.0),
            last_sample: 0.0,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let diff = input - self.last_sample;
        let slew_rate = if diff > 0.0 {
            self.pos_rate
        } else {
            self.neg_rate
        };
        let slew_limited_diff = diff.signum() * f32::min(slew_rate, diff.abs());
        self.last_sample += slew_limited_diff;

        self.last_sample
    }

    // Setter methods for parameters
    pub fn set_pos_rate(&mut self, rate: f32) {
        self.pos_rate = rate.max(0.0);
    }

    pub fn set_neg_rate(&mut self, rate: f32) {
        self.neg_rate = rate.max(0.0);
    }
}
