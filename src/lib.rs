mod compiler;
mod dsp;
mod editor;
mod gui;
mod model;
mod params;

use crate::compiler::compile_runtime_descriptor;
use crate::dsp::Engine;
use crate::params::PluginParams;
use nih_plug::prelude::*;
use std::sync::Arc;

pub struct DispersionEqualizer {
    params: Arc<PluginParams>,
    engine: Engine,
}

impl Default for DispersionEqualizer {
    fn default() -> Self {
        Self {
            params: Arc::new(PluginParams::default()),
            engine: Engine::default(),
        }
    }
}

impl Plugin for DispersionEqualizer {
    const NAME: &'static str = "Dispersion Equalizer";
    const VENDOR: &'static str = "Daishi Suzuki";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "zukky.rikugame@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        aux_input_ports: &[],
        aux_output_ports: &[],
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        editor::create(self.params.clone())
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.engine.prepare(
            buffer_config.sample_rate,
            buffer_config.max_buffer_size as usize,
        );
        self.rebuild_runtime_chain(0.0);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.rebuild_runtime_chain(0.0);
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let snapshot = self.params.runtime_snapshot();
        self.engine
            .set_mix(snapshot.wet, util::db_to_gain(snapshot.output_gain_db));

        if self.engine.needs_rebuild(&snapshot) {
            let descriptor = compile_runtime_descriptor(&snapshot, self.engine.sample_rate());
            self.engine.install_chain(snapshot, descriptor, 64.0);
        }

        for mut channel_samples in buffer.iter_samples() {
            if channel_samples.len() >= 2 {
                let input = unsafe {
                    [
                        *channel_samples.get_unchecked_mut(0),
                        *channel_samples.get_unchecked_mut(1),
                    ]
                };
                let output = self.engine.process_stereo(input);
                unsafe {
                    *channel_samples.get_unchecked_mut(0) = output[0];
                    *channel_samples.get_unchecked_mut(1) = output[1];
                }
            } else if channel_samples.len() == 1 {
                let mono = unsafe { *channel_samples.get_unchecked_mut(0) };
                let input = [mono, mono];
                let output = self.engine.process_stereo(input);
                unsafe {
                    *channel_samples.get_unchecked_mut(0) = output[0];
                }
            }
        }

        ProcessStatus::Normal
    }
}

impl DispersionEqualizer {
    fn rebuild_runtime_chain(&mut self, xfade_ms: f32) {
        let snapshot = self.params.runtime_snapshot();
        let descriptor = compile_runtime_descriptor(&snapshot, self.engine.sample_rate());
        self.engine.install_chain(snapshot, descriptor, xfade_ms);
    }
}

impl ClapPlugin for DispersionEqualizer {
    const CLAP_ID: &'static str = "com.zukky.dispersion-equalizer";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Dispersion Equalizer is a group-delay EQ that shapes the timing of sound across frequency while preserving the original tonal balance.");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for DispersionEqualizer {
    const VST3_CLASS_ID: [u8; 16] = *b"DispersionEQ!!!!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_clap!(DispersionEqualizer);
nih_export_vst3!(DispersionEqualizer);
