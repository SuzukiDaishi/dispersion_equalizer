use crate::compiler::descriptor::RuntimeChainDescriptor;
use crate::dsp::delay_line::DelayLine;
use crate::dsp::smooth::SmoothedParam;
use crate::dsp::{RuntimeChain, MAX_RUNTIME_SECTIONS};
use crate::model::RuntimeSnapshot;

/// wet チャンネルのピークリミッター: リリース時定数。
const LIMITER_RELEASE_MS: f32 = 200.0;

#[derive(Debug)]
pub struct Engine {
    sample_rate: f32,
    chain_a: RuntimeChain,
    chain_b: RuntimeChain,
    dry_delay: DelayLine,
    active_is_a: bool,
    xfade_pos: u32,
    xfade_len: u32,
    wet: SmoothedParam,
    output_gain: SmoothedParam,
    last_snapshot: Option<RuntimeSnapshot>,
    latency_samples: u32,
    /// wet チャンネルのピークリミッター用ピーク追跡値。
    limiter_level: f32,
    limiter_release_coeff: f32,
}

impl Default for Engine {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            sample_rate,
            chain_a: RuntimeChain::default(),
            chain_b: RuntimeChain::default(),
            dry_delay: DelayLine::default(),
            active_is_a: true,
            xfade_pos: 0,
            xfade_len: 0,
            wet: SmoothedParam::new(sample_rate, 10.0, 1.0),
            output_gain: SmoothedParam::new(sample_rate, 10.0, 1.0),
            last_snapshot: None,
            latency_samples: 0,
            limiter_level: 0.0,
            limiter_release_coeff: limiter_release_coeff(sample_rate),
        }
    }
}

impl Engine {
    pub fn prepare(&mut self, sample_rate: f32, _max_buffer_size: usize) {
        self.sample_rate = sample_rate.max(1.0);
        self.chain_a.prepare(self.sample_rate);
        self.chain_b.prepare(self.sample_rate);
        self.dry_delay
            .prepare_samples(self.sample_rate, MAX_RUNTIME_SECTIONS * 2 + 8);
        self.wet.set_sample_rate(self.sample_rate, 10.0);
        self.output_gain.set_sample_rate(self.sample_rate, 10.0);
        self.limiter_release_coeff = limiter_release_coeff(self.sample_rate);
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn latency_samples(&self) -> u32 {
        self.latency_samples
    }

    pub fn reset(&mut self) {
        self.chain_a.reset();
        self.chain_b.reset();
        self.dry_delay.reset();
        self.active_is_a = true;
        self.xfade_pos = 0;
        self.xfade_len = 0;
        self.wet.reset(1.0);
        self.output_gain.reset(1.0);
        self.last_snapshot = None;
        self.latency_samples = 0;
        self.limiter_level = 0.0;
    }

    pub fn set_mix(&mut self, wet: f32, output_gain: f32) {
        self.wet.set_target(wet.clamp(0.0, 1.0));
        self.output_gain.set_target(output_gain.max(0.0));
    }

    pub fn last_snapshot(&self) -> Option<RuntimeSnapshot> {
        self.last_snapshot
    }

    pub fn apply_descriptor(
        &mut self,
        snapshot: RuntimeSnapshot,
        descriptor: RuntimeChainDescriptor,
        transition_ms: f32,
        hard_change: bool,
    ) {
        let incoming_latency = descriptor.latency_samples();
        self.latency_samples = incoming_latency;
        self.dry_delay
            .set_delay_samples(incoming_latency as f32, 0.0);

        if hard_change {
            self.apply_hard_descriptor(snapshot, descriptor, transition_ms);
        } else {
            self.cancel_crossfade_without_switching();
            let immediate = self.last_snapshot.is_none();
            self.active_chain_mut()
                .apply_descriptor(&descriptor, immediate);
            self.last_snapshot = Some(snapshot);
        }
    }

    fn apply_hard_descriptor(
        &mut self,
        snapshot: RuntimeSnapshot,
        descriptor: RuntimeChainDescriptor,
        transition_ms: f32,
    ) {
        let transition_samples =
            (self.sample_rate * transition_ms.max(0.0) / 1000.0).round() as u32;
        if transition_samples == 0 || self.last_snapshot.is_none() {
            self.cancel_crossfade_without_switching();
            let active = self.active_chain_mut();
            active.reset();
            active.apply_descriptor(&descriptor, true);
            self.last_snapshot = Some(snapshot);
            return;
        }

        self.cancel_crossfade_without_switching();

        let incoming = self.inactive_chain_mut();
        incoming.reset();
        incoming.apply_descriptor(&descriptor, true);

        self.xfade_len = transition_samples.max(1);
        self.xfade_pos = 0;
        self.last_snapshot = Some(snapshot);
    }

    pub fn process_stereo(&mut self, input: [f32; 2], auto_duck: bool) -> [f32; 2] {
        let dry_frame = self.dry_delay.process(input);

        let wet_frame = if self.xfade_pos < self.xfade_len {
            let t = crossfade_position(self.xfade_pos, self.xfade_len);
            self.xfade_pos = self.xfade_pos.saturating_add(1);
            let (active_gain, incoming_gain) = linear_crossfade_gains(t);

            let a_out = self.chain_a.process(input);
            let b_out = self.chain_b.process(input);
            let (active_out, incoming_out) = if self.active_is_a {
                (a_out, b_out)
            } else {
                (b_out, a_out)
            };

            if self.xfade_pos >= self.xfade_len {
                self.active_is_a = !self.active_is_a;
                self.xfade_pos = 0;
                self.xfade_len = 0;
                self.inactive_chain_mut().reset();
            }

            [
                active_out[0] * active_gain + incoming_out[0] * incoming_gain,
                active_out[1] * active_gain + incoming_out[1] * incoming_gain,
            ]
        } else {
            self.active_chain_mut().process(input)
        };

        // ── Brickwall Limiter (wet channel) ─────────────────────────────────
        // インスタントアタック + スローリリースのピークリミッター。
        // バンチング/allpass 状態変化による振幅スパイクを 0dBFS 以内に抑える。
        let wet_gain = if auto_duck {
            let wet_peak = wet_frame[0].abs().max(wet_frame[1].abs());
            if wet_peak > self.limiter_level {
                self.limiter_level = wet_peak;
            } else {
                self.limiter_level *= self.limiter_release_coeff;
            }
            (1.0_f32 / self.limiter_level.max(1.0)).min(1.0)
        } else {
            self.limiter_level = 0.0;
            1.0
        };

        let wet = self.wet.next();
        let dry = 1.0 - wet;
        let gain = self.output_gain.next();
        [
            sanitize((dry_frame[0] * dry + wet_frame[0] * wet * wet_gain) * gain),
            sanitize((dry_frame[1] * dry + wet_frame[1] * wet * wet_gain) * gain),
        ]
    }

    fn active_chain_mut(&mut self) -> &mut RuntimeChain {
        if self.active_is_a {
            &mut self.chain_a
        } else {
            &mut self.chain_b
        }
    }

    fn inactive_chain_mut(&mut self) -> &mut RuntimeChain {
        if self.active_is_a {
            &mut self.chain_b
        } else {
            &mut self.chain_a
        }
    }

    fn cancel_crossfade_without_switching(&mut self) {
        if self.xfade_pos < self.xfade_len {
            self.inactive_chain_mut().reset();
        }
        self.xfade_pos = 0;
        self.xfade_len = 0;
    }
}

impl RuntimeChainDescriptor {
    pub fn latency_samples(&self) -> u32 {
        (self.max_sections as u32).saturating_mul(2)
    }
}

fn limiter_release_coeff(sample_rate: f32) -> f32 {
    if sample_rate <= 0.0 {
        return 0.0;
    }
    (-1.0_f32 / (LIMITER_RELEASE_MS / 1000.0 * sample_rate)).exp()
}

fn crossfade_position(pos: u32, len: u32) -> f32 {
    if len <= 1 {
        1.0
    } else {
        (pos as f32 / (len - 1) as f32).clamp(0.0, 1.0)
    }
}

fn linear_crossfade_gains(t: f32) -> (f32, f32) {
    let t = t.clamp(0.0, 1.0);
    (1.0 - t, t)
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
    use arrayvec::ArrayVec;

    fn descriptor(max_sections: usize) -> RuntimeChainDescriptor {
        RuntimeChainDescriptor {
            global_delay_ms: 0.0,
            max_sections,
            sections: ArrayVec::new(),
        }
    }

    #[test]
    fn linear_crossfade_is_bounded() {
        for step in 0..=128 {
            let t = step as f32 / 128.0;
            let (old, new) = linear_crossfade_gains(t);
            assert!((old + new - 1.0).abs() < 1e-6);
            assert!((0.0..=1.0).contains(&old));
            assert!((0.0..=1.0).contains(&new));
        }
    }

    #[test]
    fn max_sos_updates_reported_latency() {
        let mut engine = Engine::default();
        engine.prepare(48_000.0, 512);
        let mut snapshot = RuntimeSnapshot::default();
        snapshot.max_sections = 8;
        engine.apply_descriptor(snapshot, descriptor(8), 0.0, true);
        assert_eq!(engine.latency_samples(), 16);

        snapshot.max_sections = 32;
        engine.apply_descriptor(snapshot, descriptor(32), 0.0, true);
        assert_eq!(engine.latency_samples(), 64);
    }

    fn descriptor_with_allpass(global_delay_ms: f32) -> RuntimeChainDescriptor {
        use crate::compiler::descriptor::SectionDescriptor;
        let mut sections = ArrayVec::new();
        sections.push(SectionDescriptor::SecondOrder { freq_hz: 1000.0, q: 30.0 });
        sections.push(SectionDescriptor::SecondOrder { freq_hz: 3000.0, q: 30.0 });
        RuntimeChainDescriptor { global_delay_ms, max_sections: 8, sections }
    }

    /// 0dBFS 矩形波入力で delay 0→1000ms ランプ後も出力が ≤ 1.0 であること。
    #[test]
    fn no_clipping_with_noise_through_delay_ramp() {
        let mut engine = Engine::default();
        engine.prepare(48_000.0, 512);
        engine.apply_descriptor(
            RuntimeSnapshot::default(),
            descriptor_with_allpass(0.0),
            0.0, true,
        );
        for i in 0..2048 {
            let x = if i % 2 == 0 { 1.0_f32 } else { -1.0_f32 };
            engine.process_stereo([x, x], true);
        }
        engine.apply_descriptor(
            RuntimeSnapshot::default(),
            descriptor_with_allpass(1000.0),
            0.0, false,
        );
        let mut peak: f32 = 0.0;
        for i in 0..55200 {
            let x = if i % 2 == 0 { 1.0_f32 } else { -1.0_f32 };
            let out = engine.process_stereo([x, x], true);
            peak = peak.max(out[0].abs()).max(out[1].abs());
        }
        assert!(peak <= 1.0, "output clipped: peak={peak} (expected ≤ 1.0)");
    }

    /// 0dBFS サイン波（allpass 共振周波数）で delay 0→1000ms ランプ後も出力が ≤ 1.0 であること。
    #[test]
    fn no_clipping_with_sine_through_delay_ramp() {
        use std::f32::consts::PI;
        let sample_rate = 48_000.0_f32;
        let freq = 1000.0_f32;
        let mut engine = Engine::default();
        engine.prepare(sample_rate, 512);
        engine.apply_descriptor(
            RuntimeSnapshot::default(),
            descriptor_with_allpass(0.0),
            0.0, true,
        );
        for i in 0..2048 {
            let x = (2.0 * PI * freq * i as f32 / sample_rate).sin();
            engine.process_stereo([x, x], true);
        }
        engine.apply_descriptor(
            RuntimeSnapshot::default(),
            descriptor_with_allpass(1000.0),
            0.0, false,
        );
        let mut peak: f32 = 0.0;
        for i in 0..55200 {
            let x = (2.0 * PI * freq * (2048 + i) as f32 / sample_rate).sin();
            let out = engine.process_stereo([x, x], true);
            peak = peak.max(out[0].abs()).max(out[1].abs());
        }
        assert!(peak <= 1.0, "sine output clipped: peak={peak} (expected ≤ 1.0)");
    }

    /// auto_duck=true のとき 0dBFS サイン波入力が遅延ランプ中も ≤ 1.0 に保たれること。
    #[test]
    fn duck_enabled_keeps_output_bounded() {
        use std::f32::consts::PI;
        let sample_rate = 48_000.0_f32;
        let freq = 1000.0_f32;
        let mut engine = Engine::default();
        engine.prepare(sample_rate, 512);
        engine.apply_descriptor(RuntimeSnapshot::default(), descriptor_with_allpass(0.0), 0.0, true);
        for i in 0..2048 {
            let x = (2.0 * PI * freq * i as f32 / sample_rate).sin();
            engine.process_stereo([x, x], true);
        }
        engine.apply_descriptor(RuntimeSnapshot::default(), descriptor_with_allpass(1000.0), 0.0, false);
        let mut peak: f32 = 0.0;
        for i in 0..55200 {
            let x = (2.0 * PI * freq * (2048 + i) as f32 / sample_rate).sin();
            let out = engine.process_stereo([x, x], true);
            peak = peak.max(out[0].abs()).max(out[1].abs());
        }
        assert!(peak <= 1.0, "duck=on: peak={peak}");
    }

}
