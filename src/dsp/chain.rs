use crate::compiler::descriptor::{RuntimeChainDescriptor, SectionDescriptor};
use crate::dsp::allpass::SmoothSosAllpass;
use crate::dsp::delay_line::DelayLine;

pub const MAX_RUNTIME_SECTIONS: usize = 1024;

#[derive(Clone, Debug)]
pub struct RuntimeChain {
    delay: DelayLine,
    sections: Box<[SmoothSosAllpass; MAX_RUNTIME_SECTIONS]>,
    active_slots: usize,
    sample_rate: f32,
    global_delay_ms: f32,
}

impl Default for RuntimeChain {
    fn default() -> Self {
        Self {
            delay: DelayLine::default(),
            sections: Box::new([SmoothSosAllpass::default(); MAX_RUNTIME_SECTIONS]),
            active_slots: 0,
            sample_rate: 48_000.0,
            global_delay_ms: 0.0,
        }
    }
}

impl RuntimeChain {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.delay.prepare(self.sample_rate, 1000.0);
        for section in self.sections.iter_mut() {
            section.prepare(self.sample_rate);
        }
    }

    pub fn reset(&mut self) {
        self.delay.reset();
        for section in self.sections.iter_mut().take(self.active_slots) {
            section.reset_state();
        }
    }

    pub fn apply_descriptor(&mut self, descriptor: &RuntimeChainDescriptor, immediate: bool) {
        self.global_delay_ms = descriptor.global_delay_ms.clamp(0.0, 1000.0);
        self.delay
            .set_delay_ms(self.global_delay_ms, if immediate { 0.0 } else { 50.0 });

        let slots = descriptor.max_sections.min(MAX_RUNTIME_SECTIONS);
        self.active_slots = slots;

        for (slot, section) in self.sections.iter_mut().take(slots).enumerate() {
            match descriptor.sections.get(slot).copied().unwrap_or_default() {
                SectionDescriptor::SecondOrder { freq_hz, q } => {
                    section.set_target(self.sample_rate, freq_hz, q, immediate);
                }
                SectionDescriptor::Bypass => section.set_neutral_target(immediate),
            }
        }

        for section in self.sections.iter_mut().skip(slots) {
            section.set_neutral_target(true);
        }
    }

    #[cfg(test)]
    pub fn latency_samples(&self) -> u32 {
        (self.active_slots as u32).saturating_mul(2)
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut x = self.delay.process(input);
        for section in self.sections.iter_mut().take(self.active_slots) {
            x = section.process(x);
        }
        sanitize_stereo(x)
    }
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

    fn descriptor_with_slots(max_sections: usize) -> RuntimeChainDescriptor {
        RuntimeChainDescriptor {
            global_delay_ms: 0.0,
            max_sections,
            sections: Default::default(),
        }
    }

    #[test]
    fn neutral_slots_define_structural_latency() {
        let mut chain = RuntimeChain::default();
        chain.prepare(48_000.0);
        chain.apply_descriptor(&descriptor_with_slots(8), true);
        assert_eq!(chain.latency_samples(), 16);

        let mut peak_index = None;
        for index in 0..32 {
            let input = if index == 0 { [1.0, 1.0] } else { [0.0, 0.0] };
            let out = chain.process(input);
            if out[0].abs() > 0.5 {
                peak_index = Some(index);
                break;
            }
        }
        assert_eq!(peak_index, Some(16));
    }
}
