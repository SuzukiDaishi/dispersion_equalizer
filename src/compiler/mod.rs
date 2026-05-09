pub mod descriptor;
pub mod greedy;

use crate::compiler::descriptor::{PreviewCurve, RuntimeChainDescriptor, SectionDescriptor};
use crate::compiler::greedy::{group_delay_ms_one, run_greedy, target_at_freq};
use crate::model::RuntimeSnapshot;
use arrayvec::ArrayVec;

pub const MIN_FREQ_HZ: f32 = 20.0;
pub const MAX_FREQ_HZ: f32 = 20_000.0;
pub const PREVIEW_POINTS: usize = 192;

// ─── Main compile entry points ────────────────────────────────────────────────

/// Build the DSP chain descriptor from the current snapshot using greedy fitting.
pub fn compile_runtime_descriptor(
    snapshot: &RuntimeSnapshot,
    sample_rate: f32,
) -> RuntimeChainDescriptor {
    let result = run_greedy(snapshot, sample_rate);

    let mut sections: ArrayVec<SectionDescriptor, { crate::dsp::MAX_RUNTIME_SECTIONS }> =
        ArrayVec::new();
    for s in result.sections {
        if sections.is_full() {
            break;
        }
        sections.push(s);
    }

    RuntimeChainDescriptor {
        global_delay_ms: result.base_delay_ms,
        max_sections: (snapshot.max_sections as usize).min(crate::dsp::MAX_RUNTIME_SECTIONS),
        sections,
    }
}

/// Generate 192-point preview curve (target + actual from greedy fit).
pub fn compile_preview(
    snapshot: &RuntimeSnapshot,
    sample_rate: f32,
    graph_max_ms: f32,
) -> PreviewCurve {
    let result = run_greedy(snapshot, sample_rate);
    let max_ms = graph_max_ms.max(10.0);
    let top_freq = MAX_FREQ_HZ.min(sample_rate * 0.45).max(MIN_FREQ_HZ * 2.0);

    let mut target_points = Vec::with_capacity(PREVIEW_POINTS);
    let mut actual_points = Vec::with_capacity(PREVIEW_POINTS);

    for index in 0..PREVIEW_POINTS {
        let t = index as f32 / (PREVIEW_POINTS - 1) as f32;
        let freq = log_lerp(MIN_FREQ_HZ, top_freq, t);
        let target = target_at_freq(snapshot, freq).clamp(0.0, max_ms);

        // Actual = pure delay + sum of all fitted section delays
        let actual = (result.base_delay_ms
            + result
                .sections
                .iter()
                .map(|s| match *s {
                    SectionDescriptor::SecondOrder { freq_hz, q } => {
                        group_delay_ms_one(freq, freq_hz, q, sample_rate)
                    }
                    SectionDescriptor::Bypass => 0.0,
                })
                .sum::<f32>())
        .clamp(0.0, max_ms);

        target_points.push([freq, target]);
        actual_points.push([freq, actual]);
    }

    PreviewCurve {
        target_points,
        actual_points,
        fit_error_ms: result.fit_error_ms,
        section_count: result.sections.len(),
        pure_delay_ms: result.base_delay_ms,
    }
}

// Re-export helpers used by GUI/tests
pub use greedy::{node_delay_at_freq, scale_frequencies};

fn log_lerp(min: f32, max: f32, t: f32) -> f32 {
    10.0_f32.powf(min.log10() + (max.log10() - min.log10()) * t.clamp(0.0, 1.0))
}
