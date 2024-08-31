pub struct DCBlocker {
    prev_input: f32,
    prev_output: f32,
    coeff: f32,
}

impl DCBlocker {
    pub fn new(sample_rate: f32) -> Self {
        DCBlocker {
            prev_input: 0.0,
            prev_output: 0.0,
            coeff: Self::calculate_coefficient(sample_rate),
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.coeff = Self::calculate_coefficient(sample_rate);
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = input - self.prev_input + self.coeff * self.prev_output;
        self.prev_input = input;
        self.prev_output = output;
        output
    }

    fn calculate_coefficient(sample_rate: f32) -> f32 {
        // Calculate the coefficient based on a corner frequency of 20 Hz
        let corner_freq = 20.0;
        let tau = 1.0 / (2.0 * std::f32::consts::PI * corner_freq);
        (tau * sample_rate - 1.0) / (tau * sample_rate + 1.0)
    }
}
