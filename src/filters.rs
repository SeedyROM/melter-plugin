pub struct DCBlocker {
    prev_input: f32,
    prev_output: f32,
}

impl DCBlocker {
    const R: f32 = 0.995;

    pub fn new() -> Self {
        DCBlocker {
            prev_input: 0.0,
            prev_output: 0.0,
        }
    }

    pub fn process(&mut self, input: f32) -> f32 {
        let output = input - self.prev_input + Self::R * self.prev_output;
        self.prev_input = input;
        self.prev_output = output;
        output
    }
}
