use crate::compiler::descriptor::{RuntimeChainDescriptor, SectionDescriptor};
use crate::dsp::allpass::SosAllpass;
use crate::dsp::delay_line::DelayLine;

pub const MAX_RUNTIME_SECTIONS: usize = 1024;

#[derive(Clone, Copy, Debug)]
enum RuntimeSection {
    SecondOrder(SosAllpass),
    Bypass,
}

impl Default for RuntimeSection {
    fn default() -> Self {
        Self::Bypass
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeChain {
    delay: DelayLine,
    sections: Box<[RuntimeSection; MAX_RUNTIME_SECTIONS]>,
    len: usize,
    sample_rate: f32,
    global_delay_ms: f32,
}

impl Default for RuntimeChain {
    fn default() -> Self {
        Self {
            delay: DelayLine::default(),
            sections: Box::new([RuntimeSection::Bypass; MAX_RUNTIME_SECTIONS]),
            len: 0,
            sample_rate: 48_000.0,
            global_delay_ms: 0.0,
        }
    }
}

impl RuntimeChain {
    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate.max(1.0);
        self.delay.prepare(self.sample_rate, 1000.0);
    }

    pub fn reset(&mut self) {
        self.delay.reset();
        for section in self.sections.iter_mut().take(self.len) {
            match section {
                RuntimeSection::SecondOrder(filter) => filter.reset(),
                RuntimeSection::Bypass => {}
            }
        }
    }

    pub fn copy_state_from(&mut self, other: &Self) {
        self.delay.copy_state_from(&other.delay);
        *self.sections = *other.sections;
        self.len = other.len;
        self.sample_rate = other.sample_rate;
        self.global_delay_ms = other.global_delay_ms;
    }

    pub fn apply_descriptor(&mut self, descriptor: &RuntimeChainDescriptor) {
        self.global_delay_ms = descriptor.global_delay_ms.clamp(0.0, 1000.0);
        self.delay.set_delay_ms(self.global_delay_ms, 0.0);
        self.len = 0;
        *self.sections = [RuntimeSection::Bypass; MAX_RUNTIME_SECTIONS];

        for section in &descriptor.sections {
            if self.len >= MAX_RUNTIME_SECTIONS {
                break;
            }

            self.sections[self.len] = match *section {
                SectionDescriptor::SecondOrder { freq_hz, q } => {
                    let mut filter = SosAllpass::default();
                    filter.set_params(self.sample_rate, freq_hz, q);
                    RuntimeSection::SecondOrder(filter)
                }
                SectionDescriptor::Bypass => continue,
            };
            self.len += 1;
        }
    }

    pub fn process(&mut self, input: [f32; 2]) -> [f32; 2] {
        let mut x = self.delay.process(input);
        for section in self.sections.iter_mut().take(self.len) {
            x = match section {
                RuntimeSection::SecondOrder(filter) => filter.process(x),
                RuntimeSection::Bypass => x,
            };
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
