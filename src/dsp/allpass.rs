use std::f32::consts::PI;

/// Standard RBJ biquad all-pass filter (H has unit magnitude everywhere).
/// Coefficients match preview.html `biquadAllpassCoeffs()` exactly.
#[derive(Clone, Copy, Debug)]
pub struct SosAllpass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
    bypass: bool,
}

impl Default for SosAllpass {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: [0.0; 2],
            z2: [0.0; 2],
            bypass: true,
        }
    }
}

impl SosAllpass {
    /// Set allpass parameters using RBJ formula (matches preview.html).
    /// `freq_hz`: center frequency, `q`: quality factor controlling bandwidth.
    pub fn set_params(&mut self, sample_rate: f32, freq_hz: f32, q: f32) {
        if !sample_rate.is_finite() || sample_rate <= 1.0 || !freq_hz.is_finite() {
            *self = Self::default();
            return;
        }

        let freq = freq_hz.clamp(1.0, sample_rate * 0.49);
        let q_clamped = q.clamp(0.0001, 1000.0);
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q_clamped);
        let cos_w0 = w0.cos();
        let a0_inv = 1.0 / (1.0 + alpha);

        let b0 = (1.0 - alpha) * a0_inv;
        let b1 = (-2.0 * cos_w0) * a0_inv;
        let b2 = (1.0 + alpha) * a0_inv;
        // allpass identity: a1 = b1, a2 = b0
        let a1 = b1;
        let a2 = b0;

        if [b0, b1, b2, a1, a2].iter().all(|v| v.is_finite()) {
            self.b0 = b0;
            self.b1 = b1;
            self.b2 = b2;
            self.a1 = a1;
            self.a2 = a2;
            self.bypass = false;
        } else {
            *self = Self::default();
        }
    }

    pub fn reset(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        if self.bypass {
            return input;
        }

        [
            self.process_channel(input[0], 0),
            self.process_channel(input[1], 1),
        ]
    }

    fn process_channel(&mut self, x: f32, ch: usize) -> f32 {
        let y = self.b0 * x + self.z1[ch];
        self.z1[ch] = self.b1 * x - self.a1 * y + self.z2[ch];
        self.z2[ch] = self.b2 * x - self.a2 * y;
        sanitize(y)
    }
}


fn sanitize(value: f32) -> f32 {
    if value.is_finite() && value.abs() > 1e-30 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn magnitude_at(filter: SosAllpass, freq_hz: f32, sample_rate: f32) -> f32 {
        let w = 2.0 * PI * freq_hz / sample_rate;
        let c1 = (-w).cos();
        let s1 = (-w).sin();
        let c2 = (-2.0 * w).cos();
        let s2 = (-2.0 * w).sin();
        let nr = filter.b0 + filter.b1 * c1 + filter.b2 * c2;
        let ni = filter.b1 * s1 + filter.b2 * s2;
        let dr = 1.0 + filter.a1 * c1 + filter.a2 * c2;
        let di = filter.a1 * s1 + filter.a2 * s2;
        ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()
    }

    #[test]
    fn sos_magnitude_is_flat() {
        let mut filter = SosAllpass::default();
        filter.set_params(48_000.0, 1000.0, 2.0);
        for freq in [40.0, 100.0, 1000.0, 5000.0, 12000.0] {
            let mag = magnitude_at(filter, freq, 48_000.0);
            assert!((mag - 1.0).abs() < 1e-4, "{freq}: {mag}");
        }
    }

    #[test]
    fn sos_impulse_does_not_explode() {
        let mut filter = SosAllpass::default();
        filter.set_params(48_000.0, 1000.0, 5.0);
        let mut peak: f32 = 0.0;
        for index in 0..4096 {
            let input = if index == 0 { [1.0, 1.0] } else { [0.0, 0.0] };
            let out = filter.process(input);
            peak = peak.max(out[0].abs()).max(out[1].abs());
            assert!(out[0].is_finite() && out[1].is_finite());
        }
        assert!(peak < 4.0);
    }
}
