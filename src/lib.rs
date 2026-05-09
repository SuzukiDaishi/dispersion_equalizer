mod compiler;
mod dsp;
mod editor;
mod gui;
mod model;
mod params;

use crate::compiler::compile_runtime_descriptor;
use crate::compiler::descriptor::RuntimeChainDescriptor;
use crate::dsp::Engine;
use crate::model::{NodeType, RuntimeSnapshot};
use crate::params::PluginParams;
use arc_swap::ArcSwapOption;
use nih_plug::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct DispersionEqualizer {
    params: Arc<PluginParams>,
    engine: Engine,
    compile_results: Arc<ArcSwapOption<CompiledRuntime>>,
    compile_requested_sequence: Arc<AtomicU64>,
    compile_finished_sequence: Arc<AtomicU64>,
    next_compile_sequence: u64,
    scheduled_sequence: u64,
    applied_sequence: u64,
    scheduled_snapshot: Option<RuntimeSnapshot>,
    pending_compile_snapshot: Option<RuntimeSnapshot>,
    compile_in_flight: bool,
    reported_latency_samples: u32,
}

#[derive(Debug)]
struct CompiledRuntime {
    sequence: u64,
    snapshot: RuntimeSnapshot,
    descriptor: RuntimeChainDescriptor,
}

#[derive(Clone, Copy, Debug)]
pub enum CompileTask {
    Compile {
        sequence: u64,
        snapshot: RuntimeSnapshot,
        sample_rate: f32,
    },
}

impl Default for DispersionEqualizer {
    fn default() -> Self {
        Self {
            params: Arc::new(PluginParams::default()),
            engine: Engine::default(),
            compile_results: Arc::new(ArcSwapOption::from(None)),
            compile_requested_sequence: Arc::new(AtomicU64::new(0)),
            compile_finished_sequence: Arc::new(AtomicU64::new(0)),
            next_compile_sequence: 0,
            scheduled_sequence: 0,
            applied_sequence: 0,
            scheduled_snapshot: None,
            pending_compile_snapshot: None,
            compile_in_flight: false,
            reported_latency_samples: 0,
        }
    }
}

impl Plugin for DispersionEqualizer {
    const NAME: &'static str = "Dispersion Equalizer";
    const VENDOR: &'static str = "zukky";
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
    type BackgroundTask = CompileTask;

    fn task_executor(&mut self) -> TaskExecutor<Self> {
        let compile_results = self.compile_results.clone();
        let compile_requested_sequence = self.compile_requested_sequence.clone();
        let compile_finished_sequence = self.compile_finished_sequence.clone();
        Box::new(move |task| match task {
            CompileTask::Compile {
                sequence,
                snapshot,
                sample_rate,
            } => {
                let descriptor = compile_runtime_descriptor(&snapshot, sample_rate);
                if compile_requested_sequence.load(Ordering::Acquire) == sequence {
                    compile_results.store(Some(Arc::new(CompiledRuntime {
                        sequence,
                        snapshot,
                        descriptor,
                    })));
                }
                compile_finished_sequence.store(sequence, Ordering::Release);
            }
        })
    }

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
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.engine.prepare(
            buffer_config.sample_rate,
            buffer_config.max_buffer_size as usize,
        );
        self.install_initial_chain();
        context.set_latency_samples(self.engine.latency_samples());
        self.reported_latency_samples = self.engine.latency_samples();
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.install_initial_chain();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let snapshot = self.params.runtime_snapshot();
        self.engine
            .set_mix(snapshot.wet, util::db_to_gain(snapshot.output_gain_db));

        self.queue_compile_if_needed(context, self.params.target_snapshot());
        self.apply_finished_compile(context);

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
                let output = self.engine.process_stereo([mono, mono]);
                unsafe {
                    *channel_samples.get_unchecked_mut(0) = output[0];
                }
            }
        }

        ProcessStatus::Normal
    }
}

impl DispersionEqualizer {
    fn install_initial_chain(&mut self) {
        let snapshot = self.params.target_snapshot();
        let descriptor = compile_runtime_descriptor(&snapshot, self.engine.sample_rate());
        self.engine
            .apply_descriptor(snapshot, descriptor, 0.0, true);
        self.next_compile_sequence = 0;
        self.scheduled_sequence = 0;
        self.applied_sequence = 0;
        self.scheduled_snapshot = Some(snapshot);
        self.pending_compile_snapshot = None;
        self.compile_in_flight = false;
        self.compile_requested_sequence.store(0, Ordering::Release);
        self.compile_finished_sequence.store(0, Ordering::Release);
    }

    fn apply_finished_compile(&mut self, context: &mut impl ProcessContext<Self>) {
        let Some(compiled) = self.compile_results.load_full() else {
            return;
        };
        if compiled.sequence <= self.applied_sequence {
            return;
        }
        if compiled.sequence != self.compile_requested_sequence.load(Ordering::Acquire) {
            return;
        }

        let hard_change = self
            .engine
            .last_snapshot()
            .map_or(true, |last| topology_changed(&last, &compiled.snapshot));
        let transition_ms = if hard_change {
            self.params.transition_ms.value()
        } else {
            0.0
        };

        self.engine.apply_descriptor(
            compiled.snapshot,
            compiled.descriptor.clone(),
            transition_ms,
            hard_change,
        );
        self.applied_sequence = compiled.sequence;

        if compiled.sequence >= self.scheduled_sequence {
            self.compile_in_flight = false;
        }

        let latency = self.engine.latency_samples();
        if latency != self.reported_latency_samples {
            context.set_latency_samples(latency);
            self.reported_latency_samples = latency;
        }
    }

    fn queue_compile_if_needed(
        &mut self,
        context: &mut impl ProcessContext<Self>,
        target: RuntimeSnapshot,
    ) {
        if self.compile_in_flight
            && self.compile_finished_sequence.load(Ordering::Acquire) >= self.scheduled_sequence
        {
            self.compile_in_flight = false;
        }

        if self.scheduled_snapshot != Some(target) {
            self.pending_compile_snapshot = Some(target);
            self.scheduled_snapshot = Some(target);
            if self.compile_in_flight {
                self.compile_requested_sequence.store(0, Ordering::Release);
            }
        }

        if self.compile_in_flight {
            return;
        }

        if let Some(snapshot) = self.pending_compile_snapshot.take() {
            self.next_compile_sequence = self.next_compile_sequence.saturating_add(1);
            let sequence = self.next_compile_sequence;
            self.scheduled_sequence = sequence;
            self.compile_in_flight = true;
            self.compile_requested_sequence
                .store(sequence, Ordering::Release);
            context.execute_background(CompileTask::Compile {
                sequence,
                snapshot,
                sample_rate: self.engine.sample_rate(),
            });
        }
    }
}

fn topology_changed(a: &RuntimeSnapshot, b: &RuntimeSnapshot) -> bool {
    if a.max_sections != b.max_sections {
        return true;
    }

    for (left, right) in a.nodes.iter().zip(b.nodes.iter()) {
        if left.enabled != right.enabled {
            return true;
        }
        if !left.enabled {
            continue;
        }
        if left.node_type != right.node_type {
            return true;
        }
        if left.node_type == NodeType::Scale
            && (left.scale_root != right.scale_root || left.scale_mode != right.scale_mode)
        {
            return true;
        }
    }

    false
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
