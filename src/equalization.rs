// Enum to represent different types of EQ bands
#[derive(Clone, Copy)]
pub enum BandType {
    LowShelf,
    Peak,
    HighShelf,
}

// Struct to hold biquad filter coefficients
#[derive(Clone, Copy)]
pub struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    #[allow(dead_code)]
    a0: f32, // Not used in the filter, but included for completeness
    a1: f32,
    a2: f32,
}

// Struct to hold filter state variables
#[derive(Clone, Copy)]
pub struct FilterState {
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

pub struct ParametricEQ {
    sample_rate: f32,
    bands: Vec<EQBand>,
}

pub struct EQBand {
    band_type: BandType,
    freq: f32,
    gain: f32,
    q: f32,
    coeffs: BiquadCoeffs,
    state: FilterState,
}

impl ParametricEQ {
    pub fn new(sample_rate: f32) -> Self {
        ParametricEQ {
            sample_rate,
            bands: Vec::new(),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        for band in &mut self.bands {
            band.set_params(band.freq, band.gain, band.q, sample_rate);
        }
    }

    // Add a new band to the EQ
    pub fn add_band(
        &mut self,
        band_type: BandType,
        freq: f32,
        gain_db: f32,
        q: f32,
    ) -> Result<(), &'static str> {
        if self.bands.len() >= 16 {
            return Err("Maximum number of bands (16) reached");
        }
        let mut new_band = EQBand::new(band_type, freq, gain_db, q);
        new_band.set_params(freq, gain_db, q, self.sample_rate);
        self.bands.push(new_band);
        Ok(())
    }

    // Remove a band from the EQ
    pub fn remove_band(&mut self, index: usize) -> Result<(), &'static str> {
        if index >= self.bands.len() {
            return Err("Band index out of range");
        }
        self.bands.remove(index);
        Ok(())
    }

    // Set parameters for a specific band
    pub fn set_band_params(
        &mut self,
        band: usize,
        freq: f32,
        gain_db: f32,
        q: f32,
    ) -> Result<(), &'static str> {
        if band >= self.bands.len() {
            return Err("Band index out of range");
        }
        self.bands[band].set_params(freq, gain_db, q, self.sample_rate);
        Ok(())
    }

    // Process a single sample through all bands
    pub fn process(&mut self, input: f32) -> f32 {
        let mut output = input;
        for band in &mut self.bands {
            output = band.process(output);
        }
        output
    }
}

impl EQBand {
    pub fn new(band_type: BandType, freq: f32, gain: f32, q: f32) -> Self {
        EQBand {
            band_type,
            freq,
            gain,
            q,
            coeffs: BiquadCoeffs {
                b0: 1.0,
                b1: 0.0,
                b2: 0.0,
                a0: 1.0,
                a1: 0.0,
                a2: 0.0,
            },
            state: FilterState {
                x1: 0.0,
                x2: 0.0,
                y1: 0.0,
                y2: 0.0,
            },
        }
    }

    // Set parameters for the band and calculate filter coefficients
    pub fn set_params(&mut self, freq: f32, gain_db: f32, q: f32, sample_rate: f32) {
        self.freq = freq;
        let a = 10.0f32.powf(gain_db / 40.0); // Square root of the linear gain

        // Adjust Q for shelving filters
        let adjusted_q = match self.band_type {
            BandType::LowShelf | BandType::HighShelf => q * a.max(1.0),
            BandType::Peak => q,
        };
        self.q = adjusted_q;

        // Calculate omega directly without pre-warping
        let omega = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let alpha = sin_omega / (2.0 * adjusted_q);

        let (b0, b1, b2, a0, a1, a2) = match self.band_type {
            BandType::LowShelf => {
                // Use a for boost (a > 1) and 1/a for cut (a < 1)
                let (ap1, am1) = if a > 1.0 {
                    (a + 1.0, a - 1.0)
                } else {
                    let recip_a = 1.0 / a;
                    (recip_a + 1.0, recip_a - 1.0)
                };
                let ap1_cos = ap1 * cos_omega;
                let am1_cos = am1 * cos_omega;

                (
                    a * (ap1 - am1_cos + alpha),
                    2.0 * a * (am1 - ap1_cos),
                    a * (ap1 - am1_cos - alpha),
                    ap1 + am1_cos + alpha,
                    -2.0 * (am1 + ap1_cos),
                    ap1 + am1_cos - alpha,
                )
            }
            BandType::Peak => {
                let alpha_a = alpha * a;
                let alpha_div_a = alpha / a;
                (
                    1.0 + alpha_a,
                    -2.0 * cos_omega,
                    1.0 - alpha_a,
                    1.0 + alpha_div_a,
                    -2.0 * cos_omega,
                    1.0 - alpha_div_a,
                )
            }
            BandType::HighShelf => {
                // Use a for boost (a > 1) and 1/a for cut (a < 1)
                let (ap1, am1) = if a > 1.0 {
                    (a + 1.0, a - 1.0)
                } else {
                    let recip_a = 1.0 / a;
                    (recip_a + 1.0, recip_a - 1.0)
                };
                let ap1_cos = ap1 * cos_omega;
                let am1_cos = am1 * cos_omega;

                (
                    a * (ap1 + am1_cos + alpha),
                    -2.0 * a * (am1 + ap1_cos),
                    a * (ap1 + am1_cos - alpha),
                    ap1 - am1_cos + alpha,
                    2.0 * (am1 - ap1_cos),
                    ap1 - am1_cos - alpha,
                )
            }
        };

        // Normalize the coefficients by a0
        let epsilon = 1e-6; // Small value to prevent division by zero
        self.coeffs = BiquadCoeffs {
            b0: b0 / (a0 + epsilon),
            b1: b1 / (a0 + epsilon),
            b2: b2 / (a0 + epsilon),
            a0: 1.0,
            a1: a1 / (a0 + epsilon),
            a2: a2 / (a0 + epsilon),
        };
    }

    // Process a single sample through the band's filter
    pub fn process(&mut self, input: f32) -> f32 {
        let output = self.coeffs.b0 * input
            + self.coeffs.b1 * self.state.x1
            + self.coeffs.b2 * self.state.x2
            - self.coeffs.a1 * self.state.y1
            - self.coeffs.a2 * self.state.y2;

        // Update delay lines
        self.state.x2 = self.state.x1;
        self.state.x1 = input;
        self.state.y2 = self.state.y1;
        self.state.y1 = output;

        output
    }
}
