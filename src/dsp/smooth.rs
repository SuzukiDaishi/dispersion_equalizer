#[derive(Clone, Copy, Debug)]
pub struct SmoothedParam {
    current: f32,
    target: f32,
    coeff: f32,
}

impl Default for SmoothedParam {
    fn default() -> Self {
        Self {
            current: 0.0,
            target: 0.0,
            coeff: 1.0,
        }
    }
}

impl SmoothedParam {
    pub fn new(sample_rate: f32, time_ms: f32, value: f32) -> Self {
        Self {
            current: value,
            target: value,
            coeff: smoothing_coeff(sample_rate, time_ms),
        }
    }

    pub fn reset(&mut self, value: f32) {
        self.current = value;
        self.target = value;
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32, time_ms: f32) {
        self.coeff = smoothing_coeff(sample_rate, time_ms);
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = if target.is_finite() {
            target
        } else {
            self.current
        };
    }

    pub fn next(&mut self) -> f32 {
        self.current += (self.target - self.current) * self.coeff;
        if (self.target - self.current).abs() < 1e-6 {
            self.current = self.target;
        }
        self.current
    }
}

pub fn smoothing_coeff(sample_rate: f32, time_ms: f32) -> f32 {
    if sample_rate <= 0.0 || time_ms <= 0.0 {
        return 1.0;
    }
    let time_sec = time_ms / 1000.0;
    1.0 - (-1.0 / (time_sec * sample_rate)).exp()
}
