#[derive(Clone, Debug)]
pub struct DelayLine {
    buffer: Vec<[f32; 2]>,
    write_pos: usize,
    sample_rate: f32,
    current_delay_samples: f32,
    target_delay_samples: f32,
    coeff: f32,
}

impl Default for DelayLine {
    fn default() -> Self {
        Self {
            buffer: vec![[0.0; 2]; 8],
            write_pos: 0,
            sample_rate: 48_000.0,
            current_delay_samples: 0.0,
            target_delay_samples: 0.0,
            coeff: smoothing_coeff(48_000.0, 50.0),
        }
    }
}

impl DelayLine {
    pub fn prepare(&mut self, sample_rate: f32, max_delay_ms: f32) {
        self.sample_rate = sample_rate.max(1.0);
        let max_delay_samples = (self.sample_rate * max_delay_ms / 1000.0).ceil() as usize;
        self.prepare_samples(self.sample_rate, max_delay_samples);
    }

    pub fn prepare_samples(&mut self, sample_rate: f32, max_delay_samples: usize) {
        self.sample_rate = sample_rate.max(1.0);
        let len = max_delay_samples.saturating_add(8).max(8);
        self.buffer.resize(len, [0.0; 2]);
        self.coeff = smoothing_coeff(self.sample_rate, 50.0);
        self.reset();
    }

    pub fn reset(&mut self) {
        for sample in &mut self.buffer {
            *sample = [0.0; 2];
        }
        self.write_pos = 0;
        self.current_delay_samples = self.target_delay_samples;
    }

    pub fn set_delay_ms(&mut self, delay_ms: f32, smooth_ms: f32) {
        let samples = delay_ms.max(0.0) * self.sample_rate / 1000.0;
        self.set_delay_samples(samples, smooth_ms);
    }

    pub fn set_delay_samples(&mut self, delay_samples: f32, smooth_ms: f32) {
        let max_delay = (self.buffer.len().saturating_sub(4)) as f32;
        let new_delay = delay_samples.max(0.0).clamp(0.0, max_delay);
        self.target_delay_samples = new_delay;
        self.coeff = smoothing_coeff(self.sample_rate, smooth_ms);
        if smooth_ms <= 0.0 || (new_delay - self.current_delay_samples).abs() < 0.5 {
            self.current_delay_samples = new_delay;
        }
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.buffer[self.write_pos] = input;

        self.current_delay_samples +=
            (self.target_delay_samples - self.current_delay_samples) * self.coeff;
        if (self.target_delay_samples - self.current_delay_samples).abs() < 1e-4 {
            self.current_delay_samples = self.target_delay_samples;
        }

        let output = self.read(self.current_delay_samples);

        self.write_pos += 1;
        if self.write_pos >= self.buffer.len() {
            self.write_pos = 0;
        }

        sanitize_stereo(output)
    }

    fn read(&self, delay_samples: f32) -> [f32; 2] {
        let len = self.buffer.len() as f32;
        let read_pos = (self.write_pos as f32 - delay_samples).rem_euclid(len);
        let i0 = read_pos.floor() as usize % self.buffer.len();
        let i1 = (i0 + 1) % self.buffer.len();
        let frac = read_pos - read_pos.floor();
        let a = self.buffer[i0];
        let b = self.buffer[i1];
        [a[0] + (b[0] - a[0]) * frac, a[1] + (b[1] - a[1]) * frac]
    }
}

fn smoothing_coeff(sample_rate: f32, time_ms: f32) -> f32 {
    if sample_rate <= 0.0 || time_ms <= 0.0 {
        return 1.0;
    }
    let time_sec = time_ms / 1000.0;
    1.0 - (-1.0 / (time_sec * sample_rate)).exp()
}

fn sanitize_stereo(x: [f32; 2]) -> [f32; 2] {
    [
        if x[0].is_finite() { x[0] } else { 0.0 },
        if x[1].is_finite() { x[1] } else { 0.0 },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_delay_returns_current_sample() {
        let mut delay = DelayLine::default();
        delay.prepare(48_000.0, 1000.0);
        delay.set_delay_ms(0.0, 0.0);
        assert_eq!(delay.process([1.0, -1.0]), [1.0, -1.0]);
    }

    #[test]
    fn fixed_delay_returns_later() {
        let mut delay = DelayLine::default();
        delay.prepare(10.0, 1000.0);
        delay.set_delay_ms(100.0, 0.0);
        assert_eq!(delay.process([1.0, 0.0])[0], 0.0);
        assert_eq!(delay.process([0.0, 0.0])[0], 1.0);
    }

    #[test]
    fn sample_delay_returns_after_exact_samples() {
        let mut delay = DelayLine::default();
        delay.prepare_samples(48_000.0, 32);
        delay.set_delay_samples(7.0, 0.0);
        for index in 0..7 {
            let input = if index == 0 { [1.0, 0.0] } else { [0.0, 0.0] };
            assert_eq!(delay.process(input)[0], 0.0);
        }
        assert_eq!(delay.process([0.0, 0.0])[0], 1.0);
    }
}
