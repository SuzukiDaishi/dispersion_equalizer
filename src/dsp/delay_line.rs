#[derive(Clone, Debug)]
pub struct DelayLine {
    buffer: Vec<[f32; 2]>,
    write_pos: usize,
    sample_rate: f32,
    current_delay_samples: f32,
    old_delay_samples: f32,
    target_delay_samples: f32,
    xfade_pos: u32,
    xfade_len: u32,
}

impl Default for DelayLine {
    fn default() -> Self {
        Self {
            buffer: vec![[0.0; 2]; 8],
            write_pos: 0,
            sample_rate: 48_000.0,
            current_delay_samples: 0.0,
            old_delay_samples: 0.0,
            target_delay_samples: 0.0,
            xfade_pos: 0,
            xfade_len: 1,
        }
    }
}

impl DelayLine {
    pub fn prepare(&mut self, sample_rate: f32, max_delay_ms: f32) {
        self.sample_rate = sample_rate.max(1.0);
        let len = ((self.sample_rate * max_delay_ms / 1000.0).ceil() as usize + 8).max(8);
        self.buffer.resize(len, [0.0; 2]);
        self.reset();
    }

    pub fn reset(&mut self) {
        for sample in &mut self.buffer {
            *sample = [0.0; 2];
        }
        self.write_pos = 0;
        self.current_delay_samples = self.target_delay_samples;
        self.old_delay_samples = self.target_delay_samples;
        self.xfade_pos = self.xfade_len;
    }

    pub fn copy_state_from(&mut self, other: &Self) {
        if self.buffer.len() == other.buffer.len() {
            self.buffer.copy_from_slice(&other.buffer);
        } else {
            self.buffer.resize(other.buffer.len(), [0.0; 2]);
            self.buffer.copy_from_slice(&other.buffer);
        }
        self.write_pos = other.write_pos;
        self.sample_rate = other.sample_rate;
        self.current_delay_samples = other.current_delay_samples;
        self.old_delay_samples = other.old_delay_samples;
        self.target_delay_samples = other.target_delay_samples;
        self.xfade_pos = other.xfade_pos;
        self.xfade_len = other.xfade_len;
    }

    pub fn set_delay_ms(&mut self, delay_ms: f32, xfade_ms: f32) {
        let max_delay = (self.buffer.len().saturating_sub(4)) as f32;
        let new_delay = (delay_ms.max(0.0) * self.sample_rate / 1000.0).clamp(0.0, max_delay);
        if (new_delay - self.target_delay_samples).abs() < 0.5 {
            return;
        }

        self.old_delay_samples = self.current_delay_samples;
        self.target_delay_samples = new_delay;
        self.xfade_len = ((self.sample_rate * xfade_ms.max(0.0) / 1000.0).round() as u32).max(1);
        if xfade_ms <= 0.0 {
            self.current_delay_samples = new_delay;
            self.old_delay_samples = new_delay;
            self.xfade_pos = self.xfade_len;
        } else {
            self.xfade_pos = 0;
        }
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.buffer[self.write_pos] = input;

        let output = if self.xfade_pos < self.xfade_len {
            let t = self.xfade_pos as f32 / self.xfade_len as f32;
            let old = self.read(self.old_delay_samples);
            let new = self.read(self.target_delay_samples);
            self.current_delay_samples =
                self.old_delay_samples + (self.target_delay_samples - self.old_delay_samples) * t;
            self.xfade_pos = self.xfade_pos.saturating_add(1);
            equal_power_mix(old, new, t)
        } else {
            self.current_delay_samples = self.target_delay_samples;
            self.read(self.target_delay_samples)
        };

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

fn equal_power_mix(old: [f32; 2], new: [f32; 2], t: f32) -> [f32; 2] {
    let old_g = (t * std::f32::consts::FRAC_PI_2).cos();
    let new_g = (t * std::f32::consts::FRAC_PI_2).sin();
    [
        old[0] * old_g + new[0] * new_g,
        old[1] * old_g + new[1] * new_g,
    ]
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
}
