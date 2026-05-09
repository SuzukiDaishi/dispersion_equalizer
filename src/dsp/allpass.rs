use std::f32::consts::PI;

const PARAM_UPDATE_INTERVAL: u8 = 16;
const DEFAULT_SMOOTH_MS: f32 = 35.0;
const MIN_FREQ_HZ: f32 = 1.0;
const MAX_RADIUS: f32 = 0.9995;

#[derive(Clone, Copy, Debug)]
pub struct SmoothSosAllpass {
    sample_rate: f32,
    log_freq: f32,
    target_log_freq: f32,
    radius: f32,
    target_radius: f32,
    smooth_coeff: f32,
    update_countdown: u8,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: [f32; 2],
    z2: [f32; 2],
}

impl Default for SmoothSosAllpass {
    fn default() -> Self {
        let mut filter = Self {
            sample_rate: 48_000.0,
            log_freq: 1000.0_f32.ln(),
            target_log_freq: 1000.0_f32.ln(),
            radius: 0.0,
            target_radius: 0.0,
            smooth_coeff: smoothing_coeff(48_000.0, DEFAULT_SMOOTH_MS),
            update_countdown: 0,
            b0: 0.0,
            b1: 0.0,
            b2: 1.0,
            a1: 0.0,
            a2: 0.0,
            z1: [0.0; 2],
            z2: [0.0; 2],
        };
        filter.update_coefficients();
        filter
    }
}

impl SmoothSosAllpass {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.smooth_coeff = smoothing_coeff(self.sample_rate, DEFAULT_SMOOTH_MS);
        self.target_log_freq = sanitize_log_freq(self.target_log_freq, self.sample_rate);
        self.log_freq = sanitize_log_freq(self.log_freq, self.sample_rate);
        self.radius = self.radius.clamp(0.0, MAX_RADIUS);
        self.target_radius = self.target_radius.clamp(0.0, MAX_RADIUS);
        self.update_coefficients();
    }

    pub fn reset_state(&mut self) {
        self.z1 = [0.0; 2];
        self.z2 = [0.0; 2];
    }

    pub fn set_neutral_target(&mut self, immediate: bool) {
        self.target_radius = 0.0;
        if immediate {
            self.radius = 0.0;
            self.update_coefficients();
        }
    }

    pub fn set_target(&mut self, sample_rate: f32, freq_hz: f32, q: f32, immediate: bool) {
        self.sample_rate = sample_rate.max(1.0);
        self.smooth_coeff = smoothing_coeff(self.sample_rate, DEFAULT_SMOOTH_MS);
        let target = pole_params_from_freq_q(self.sample_rate, freq_hz, q);
        self.target_log_freq = target.log_freq;
        self.target_radius = target.radius;

        if immediate {
            self.log_freq = self.target_log_freq;
            self.radius = self.target_radius;
            self.update_coefficients();
        }
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        self.update_smoothed_params();
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

    fn update_smoothed_params(&mut self) {
        if self.update_countdown > 0 {
            self.update_countdown -= 1;
            return;
        }

        self.log_freq += (self.target_log_freq - self.log_freq) * self.smooth_coeff;
        self.radius += (self.target_radius - self.radius) * self.smooth_coeff;

        if (self.target_log_freq - self.log_freq).abs() < 1e-6 {
            self.log_freq = self.target_log_freq;
        }
        if (self.target_radius - self.radius).abs() < 1e-6 {
            self.radius = self.target_radius;
        }

        self.update_coefficients();
        self.update_countdown = PARAM_UPDATE_INTERVAL.saturating_sub(1);
    }

    fn update_coefficients(&mut self) {
        let log_freq = sanitize_log_freq(self.log_freq, self.sample_rate);
        let freq = log_freq.exp();
        let theta = (2.0 * PI * freq / self.sample_rate).clamp(0.0, PI);
        let radius = self.radius.clamp(0.0, MAX_RADIUS);
        let radius_sq = radius * radius;
        let pole_term = -2.0 * radius * theta.cos();

        self.b0 = radius_sq;
        self.b1 = pole_term;
        self.b2 = 1.0;
        self.a1 = pole_term;
        self.a2 = radius_sq;

        if ![self.b0, self.b1, self.b2, self.a1, self.a2]
            .iter()
            .all(|v| v.is_finite())
        {
            *self = Self::default();
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PoleParams {
    log_freq: f32,
    radius: f32,
}

pub fn pole_radius_from_freq_q(sample_rate: f32, freq_hz: f32, q: f32) -> f32 {
    let sample_rate = sample_rate.max(1.0);
    let freq = freq_hz.clamp(MIN_FREQ_HZ, sample_rate * 0.49);
    let q = q.clamp(0.05, 500.0);
    (-PI * freq / (q * sample_rate))
        .exp()
        .clamp(0.0, MAX_RADIUS)
}

fn pole_params_from_freq_q(sample_rate: f32, freq_hz: f32, q: f32) -> PoleParams {
    let sample_rate = sample_rate.max(1.0);
    let freq = freq_hz.clamp(MIN_FREQ_HZ, sample_rate * 0.49);
    PoleParams {
        log_freq: freq.ln(),
        radius: pole_radius_from_freq_q(sample_rate, freq, q),
    }
}

fn sanitize_log_freq(log_freq: f32, sample_rate: f32) -> f32 {
    let min = MIN_FREQ_HZ.ln();
    let max = (sample_rate.max(1.0) * 0.49).max(MIN_FREQ_HZ).ln();
    if log_freq.is_finite() {
        log_freq.clamp(min, max)
    } else {
        1000.0_f32.ln().clamp(min, max)
    }
}

fn smoothing_coeff(sample_rate: f32, time_ms: f32) -> f32 {
    if sample_rate <= 0.0 || time_ms <= 0.0 {
        return 1.0;
    }
    let samples = (sample_rate * time_ms / 1000.0).max(1.0);
    1.0 - (-(PARAM_UPDATE_INTERVAL as f32) / samples).exp()
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

    fn magnitude_at(filter: SmoothSosAllpass, freq_hz: f32, sample_rate: f32) -> f32 {
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
        let mut filter = SmoothSosAllpass::default();
        filter.set_target(48_000.0, 1000.0, 2.0, true);
        for freq in [40.0, 100.0, 1000.0, 5000.0, 12_000.0] {
            let mag = magnitude_at(filter, freq, 48_000.0);
            assert!((mag - 1.0).abs() < 1e-4, "{freq}: {mag}");
        }
    }

    #[test]
    fn neutral_allpass_is_two_sample_delay() {
        let mut filter = SmoothSosAllpass::default();
        filter.set_neutral_target(true);
        let mut out = Vec::new();
        for index in 0..5 {
            let input = if index == 0 { [1.0, -1.0] } else { [0.0, 0.0] };
            out.push(filter.process(input));
        }
        assert_eq!(out[0], [0.0, 0.0]);
        assert_eq!(out[1], [0.0, 0.0]);
        assert_eq!(out[2], [1.0, -1.0]);
    }

    #[test]
    fn sos_impulse_does_not_explode() {
        let mut filter = SmoothSosAllpass::default();
        filter.set_target(48_000.0, 1000.0, 5.0, true);
        let mut peak: f32 = 0.0;
        for index in 0..4096 {
            let input = if index == 0 { [1.0, 1.0] } else { [0.0, 0.0] };
            let out = filter.process(input);
            peak = peak.max(out[0].abs()).max(out[1].abs());
            assert!(out[0].is_finite() && out[1].is_finite());
        }
        assert!(peak < 4.0, "peak={peak}");
    }

    #[test]
    fn parameter_sweep_stays_finite() {
        let mut filter = SmoothSosAllpass::default();
        filter.set_target(48_000.0, 80.0, 0.2, true);
        let mut peak: f32 = 0.0;
        for index in 0..20_000 {
            if index % 257 == 0 {
                let t = index as f32 / 20_000.0;
                let freq = 40.0 * 2.0_f32.powf(t * 9.0);
                let q = 0.1 + 80.0 * (1.0 - t);
                filter.set_target(48_000.0, freq, q, false);
            }
            let x = (index as f32 * 0.03).sin() * 0.1;
            let out = filter.process([x, -x]);
            peak = peak.max(out[0].abs()).max(out[1].abs());
            assert!(out[0].is_finite() && out[1].is_finite());
        }
        assert!(peak < 1.0, "peak={peak}");
    }
}
