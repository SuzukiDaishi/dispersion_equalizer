use crate::compiler::descriptor::RuntimeChainDescriptor;
use crate::dsp::crossfade::Crossfade;
use crate::dsp::smooth::SmoothedParam;
use crate::dsp::RuntimeChain;
use crate::model::RuntimeSnapshot;

#[derive(Debug)]
pub struct Engine {
    sample_rate: f32,
    active_chain: RuntimeChain,
    fading_chain: RuntimeChain,
    has_fading_chain: bool,
    crossfade: Crossfade,
    wet: SmoothedParam,
    output_gain: SmoothedParam,
    last_snapshot: Option<RuntimeSnapshot>,
}

impl Default for Engine {
    fn default() -> Self {
        let sample_rate = 48_000.0;
        Self {
            sample_rate,
            active_chain: RuntimeChain::default(),
            fading_chain: RuntimeChain::default(),
            has_fading_chain: false,
            crossfade: Crossfade::default(),
            wet: SmoothedParam::new(sample_rate, 10.0, 1.0),
            output_gain: SmoothedParam::new(sample_rate, 10.0, 1.0),
            last_snapshot: None,
        }
    }
}

impl Engine {
    pub fn prepare(&mut self, sample_rate: f32, _max_buffer_size: usize) {
        self.sample_rate = sample_rate.max(1.0);
        self.active_chain.prepare(self.sample_rate);
        self.fading_chain.prepare(self.sample_rate);
        self.wet.set_sample_rate(self.sample_rate, 10.0);
        self.output_gain.set_sample_rate(self.sample_rate, 10.0);
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub fn reset(&mut self) {
        self.active_chain.reset();
        self.fading_chain.reset();
        self.has_fading_chain = false;
        self.crossfade.reset();
        self.wet.reset(1.0);
        self.output_gain.reset(1.0);
        self.last_snapshot = None;
    }

    pub fn set_mix(&mut self, wet: f32, output_gain: f32) {
        self.wet.set_target(wet.clamp(0.0, 1.0));
        self.output_gain.set_target(output_gain.max(0.0));
    }

    pub fn needs_rebuild(&self, snapshot: &RuntimeSnapshot) -> bool {
        self.last_snapshot
            .as_ref()
            .map_or(true, |last| last != snapshot)
    }

    pub fn install_chain(
        &mut self,
        snapshot: RuntimeSnapshot,
        descriptor: RuntimeChainDescriptor,
        xfade_ms: f32,
    ) {
        if xfade_ms > 0.0 && self.last_snapshot.is_some() {
            self.fading_chain.copy_state_from(&self.active_chain);
            self.has_fading_chain = true;
            self.crossfade.start(self.sample_rate, xfade_ms);
        } else {
            self.has_fading_chain = false;
            self.crossfade.reset();
        }

        self.active_chain.apply_descriptor(&descriptor);
        self.last_snapshot = Some(snapshot);
    }

    pub fn process_stereo(&mut self, input: [f32; 2]) -> [f32; 2] {
        let wet_frame = if self.has_fading_chain && self.crossfade.is_active() {
            let old = self.fading_chain.process(input);
            let new = self.active_chain.process(input);
            let t = self.crossfade.next();
            let old_g = (t * std::f32::consts::FRAC_PI_2).cos();
            let new_g = (t * std::f32::consts::FRAC_PI_2).sin();
            [
                old[0] * old_g + new[0] * new_g,
                old[1] * old_g + new[1] * new_g,
            ]
        } else {
            self.has_fading_chain = false;
            self.active_chain.process(input)
        };

        let wet = self.wet.next();
        let dry = 1.0 - wet;
        let gain = self.output_gain.next();
        [
            sanitize((input[0] * dry + wet_frame[0] * wet) * gain),
            sanitize((input[1] * dry + wet_frame[1] * wet) * gain),
        ]
    }
}

fn sanitize(value: f32) -> f32 {
    if value.is_finite() && value.abs() > 1e-30 {
        value
    } else {
        0.0
    }
}
