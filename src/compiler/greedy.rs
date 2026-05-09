/// Greedy all-pass fitting algorithm — exact port of preview.html `buildPatch()`.
///
/// Given a target group-delay curve (defined by the node list), finds the optimal
/// combination of biquad all-pass sections (up to `max_sections`) that minimises
/// the weighted squared error on a 128-point log-frequency grid.
use std::f32::consts::PI;

use crate::compiler::descriptor::SectionDescriptor;
use crate::model::{NodeRuntimeParams, NodeType, RootNote, RuntimeSnapshot, ScaleMode};

const GRID_N: usize = 128;
const MIN_FREQ: f32 = 20.0;
const MAX_FREQ: f32 = 20_000.0;
/// Q values tested for every candidate centre frequency (matches preview.html).
const BASE_QS: [f32; 7] = [0.35, 0.7, 1.4, 3.0, 7.0, 16.0, 42.0];

pub struct GreedyResult {
    pub base_delay_ms: f32,
    pub sections: Vec<SectionDescriptor>,
    pub fit_error_ms: f32,
}

struct Candidate {
    freq: f32,
    q: f32,
    curve: [f32; GRID_N],
    energy: f32,
}

struct CandidateCenter {
    freq: f32,
    preferred_q: Option<f32>,
}

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn run_greedy(snapshot: &RuntimeSnapshot, sample_rate: f32) -> GreedyResult {
    let top_freq = MAX_FREQ.min(sample_rate * 0.45);

    // 128-point log-frequency grid (matches preview.html)
    let freqs: [f32; GRID_N] = std::array::from_fn(|k| {
        let t = k as f32 / (GRID_N - 1) as f32;
        log_lerp(MIN_FREQ, top_freq, t)
    });

    // Target group delay at each grid point
    let target: [f32; GRID_N] =
        std::array::from_fn(|k| target_at_freq(snapshot, freqs[k]));

    // Pure delay = minimum of target (extracted as global delay)
    let base_delay_ms = target
        .iter()
        .cloned()
        .fold(f32::INFINITY, f32::min)
        .clamp(0.0, 1000.0);

    // Residual after subtracting pure delay
    let residual: [f32; GRID_N] =
        std::array::from_fn(|k| (target[k] - base_delay_ms).max(0.0));

    // Build candidate (freq, Q) pairs
    let centers = build_candidate_centers(snapshot, sample_rate, top_freq);
    let candidates = build_candidates(&centers, &freqs, sample_rate);

    // ── Greedy loop ──────────────────────────────────────────────────────────
    let max_iter = snapshot.max_sections as usize;
    let mut actual = [0.0_f32; GRID_N];
    let mut counts = vec![0_u32; candidates.len()];

    for _ in 0..max_iter {
        let mut best_idx: Option<usize> = None;
        let mut best_improvement = 0.0_f32;

        for (i, cand) in candidates.iter().enumerate() {
            let mut dot = 0.0_f32;
            for k in 0..GRID_N {
                dot += (residual[k] - actual[k]) * cand.curve[k];
            }
            let improvement = 2.0 * dot - cand.energy;
            if improvement > best_improvement {
                best_improvement = improvement;
                best_idx = Some(i);
            }
        }

        match best_idx {
            None => break,
            Some(idx) => {
                counts[idx] += 1;
                for k in 0..GRID_N {
                    actual[k] += candidates[idx].curve[k];
                }
            }
        }
    }

    // ── Convert counts → SectionDescriptor list (unrolled) ──────────────────
    let mut sections = Vec::new();
    for (cand, &count) in candidates.iter().zip(counts.iter()) {
        for _ in 0..count {
            if sections.len() >= crate::dsp::MAX_RUNTIME_SECTIONS {
                break;
            }
            sections.push(SectionDescriptor::SecondOrder {
                freq_hz: cand.freq,
                q: cand.q,
            });
        }
        if sections.len() >= crate::dsp::MAX_RUNTIME_SECTIONS {
            break;
        }
    }

    // ── Fit RMS error ────────────────────────────────────────────────────────
    let err_sq: f32 = (0..GRID_N)
        .map(|k| {
            let e = residual[k] - actual[k];
            e * e
        })
        .sum::<f32>()
        / GRID_N as f32;
    let fit_error_ms = err_sq.sqrt();

    GreedyResult {
        base_delay_ms,
        sections,
        fit_error_ms,
    }
}

// ─── Candidate construction ───────────────────────────────────────────────────

fn build_candidate_centers(
    snapshot: &RuntimeSnapshot,
    _sample_rate: f32,
    top_freq: f32,
) -> Vec<CandidateCenter> {
    // Key = freq rounded to 2 decimal places for dedup
    let mut map: std::collections::HashMap<u64, CandidateCenter> =
        std::collections::HashMap::new();

    let add = |map: &mut std::collections::HashMap<u64, CandidateCenter>,
               f: f32,
               pq: Option<f32>| {
        let cf = f.clamp(MIN_FREQ, top_freq);
        let key = (cf * 100.0).round() as u64;
        let entry = map.entry(key).or_insert(CandidateCenter {
            freq: cf,
            preferred_q: None,
        });
        if pq.is_some() {
            entry.preferred_q = pq;
        }
    };

    // 36-point log grid
    for i in 0..36 {
        let t = i as f32 / 35.0;
        add(
            &mut map,
            10.0_f32.powf(lerp(MIN_FREQ.log10(), top_freq.log10(), t)),
            None,
        );
    }

    // Node-specific centres
    for node in snapshot.nodes.iter() {
        if !node.enabled {
            continue;
        }
        match node.node_type {
            NodeType::Scale => {
                let q = oct_bandwidth_to_q(node.width_oct * 2.2);
                for f in scale_frequencies(node) {
                    if f >= MIN_FREQ && f <= top_freq {
                        add(&mut map, f, Some(q));
                    }
                }
            }
            _ => {
                let q = oct_bandwidth_to_q(node.width_oct);
                add(&mut map, node.freq_hz, Some(q));
                let half = node.width_oct * 0.5;
                add(
                    &mut map,
                    node.freq_hz / 2.0_f32.powf(half),
                    Some(q * 0.75),
                );
                add(
                    &mut map,
                    node.freq_hz * 2.0_f32.powf(half),
                    Some(q * 0.75),
                );
            }
        }
    }

    let mut centers: Vec<CandidateCenter> = map.into_values().collect();
    centers.sort_by(|a, b| a.freq.partial_cmp(&b.freq).unwrap());
    centers
}

fn build_candidates(
    centers: &[CandidateCenter],
    freqs: &[f32; GRID_N],
    sample_rate: f32,
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for c in centers {
        // Collect Q values: preferred first, then base set; deduplicate
        let mut qs: Vec<f32> = Vec::new();
        if let Some(pq) = c.preferred_q {
            qs.push((pq.clamp(0.08, 240.0) * 1000.0).round() / 1000.0);
        }
        for &bq in &BASE_QS {
            let rounded = (bq * 1000.0).round() / 1000.0;
            if !qs.contains(&rounded) {
                qs.push(rounded);
            }
        }

        for q in qs {
            let curve: [f32; GRID_N] =
                std::array::from_fn(|k| group_delay_ms_one(freqs[k], c.freq, q, sample_rate));
            let energy: f32 = curve.iter().map(|&v| v * v).sum();
            if energy > 1e-9 {
                out.push(Candidate {
                    freq: c.freq,
                    q,
                    curve,
                    energy,
                });
            }
        }
    }
    out
}

// ─── Node shape functions (matches preview.html) ──────────────────────────────

pub fn node_delay_at_freq(node: &NodeRuntimeParams, freq_hz: f32) -> f32 {
    if !node.enabled || node.amount_ms <= 0.0 {
        return 0.0;
    }
    let shape = match node.node_type {
        NodeType::Bell => bell_shape(freq_hz, node.freq_hz, node.width_oct),
        NodeType::LowShelf => low_shelf_shape(freq_hz, node.freq_hz, node.width_oct),
        NodeType::HighShelf => high_shelf_shape(freq_hz, node.freq_hz, node.width_oct),
        NodeType::Scale => scale_shape(freq_hz, node),
    };
    node.amount_ms * shape
}

pub fn target_at_freq(snapshot: &RuntimeSnapshot, freq_hz: f32) -> f32 {
    let mut v = snapshot.global_delay_ms;
    for node in snapshot.nodes.iter() {
        v += node_delay_at_freq(node, freq_hz);
    }
    v.clamp(0.0, 1000.0)
}

fn bell_shape(freq: f32, center: f32, width_oct: f32) -> f32 {
    let sigma = width_oct.max(0.03) / 2.355;
    let d = (freq / center.max(1.0)).log2();
    (-0.5 * (d / sigma) * (d / sigma)).exp()
}

fn low_shelf_shape(freq: f32, cutoff: f32, width_oct: f32) -> f32 {
    let x = (freq / cutoff.max(1.0)).log2() / width_oct.max(0.03);
    let hi = 1.0 / (1.0 + (-5.4 * x).exp());
    1.0 - hi
}

fn high_shelf_shape(freq: f32, cutoff: f32, width_oct: f32) -> f32 {
    let x = (freq / cutoff.max(1.0)).log2() / width_oct.max(0.03);
    1.0 / (1.0 + (-5.4 * x).exp())
}

fn scale_shape(freq: f32, node: &NodeRuntimeParams) -> f32 {
    let sigma = node.width_oct.max(0.01);
    scale_frequencies(node)
        .iter()
        .map(|&sf| {
            let d = (freq / sf.max(1.0)).log2().abs();
            (-0.5 * (d / sigma) * (d / sigma)).exp()
        })
        .fold(0.0_f32, f32::max)
}

// ─── Scale frequency generation (matches preview.html scaleFrequencies()) ────

pub fn scale_frequencies(node: &NodeRuntimeParams) -> Vec<f32> {
    let intervals: &[i32] = match node.scale_mode {
        ScaleMode::MajorPentatonic => &[0, 2, 4, 7, 9],
        _ => &[0, 3, 5, 7, 10], // MinorPentatonic and others fall back to minor
    };
    let root = root_note_semitone(node.scale_root);
    let mut out = Vec::new();
    for octave in -2_i32..=10 {
        let base_midi = 12 * (octave + 1) + root;
        for &iv in intervals {
            let midi = base_midi + iv;
            let f = 440.0 * 2.0_f32.powf((midi - 69) as f32 / 12.0);
            if f >= MIN_FREQ * 0.75 && f <= MAX_FREQ * 1.25 {
                out.push(f);
            }
        }
    }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap());
    out
}

fn root_note_semitone(root: RootNote) -> i32 {
    match root {
        RootNote::C => 0,
        RootNote::CSharp => 1,
        RootNote::D => 2,
        RootNote::DSharp => 3,
        RootNote::E => 4,
        RootNote::F => 5,
        RootNote::FSharp => 6,
        RootNote::G => 7,
        RootNote::GSharp => 8,
        RootNote::A => 9,
        RootNote::ASharp => 10,
        RootNote::B => 11,
    }
}

// ─── DSP math (matches preview.html exactly) ─────────────────────────────────

/// Group delay (ms) of one RBJ biquad all-pass at frequency `freq`.
/// Uses finite-difference phase derivative — same as preview.html `groupDelayMsOne()`.
pub fn group_delay_ms_one(freq: f32, center: f32, q: f32, sample_rate: f32) -> f32 {
    let df = (freq * 0.0015).clamp(0.25, 40.0);
    let f_a = (freq - df).clamp(1.0, sample_rate * 0.49);
    let f_b = (freq + df).clamp(1.0, sample_rate * 0.49);
    let p_a = rbj_allpass_phase(f_a, center, q, sample_rate);
    let p_b = rbj_allpass_phase(f_b, center, q, sample_rate);
    let d_phi = unwrap_delta(p_b - p_a);
    let d_omega = 2.0 * PI * (f_b - f_a) / sample_rate;
    (-d_phi / d_omega).max(0.0) / sample_rate * 1000.0
}

/// Phase of H(e^jω) for RBJ biquad all-pass at evaluation frequency `freq_hz`.
fn rbj_allpass_phase(freq_hz: f32, center_hz: f32, q: f32, sample_rate: f32) -> f32 {
    // Coefficients at centre frequency
    let w0 = 2.0 * PI * center_hz.clamp(1.0, sample_rate * 0.49) / sample_rate;
    let alpha = w0.sin() / (2.0 * q.clamp(0.0001, 1000.0));
    let cos_w0 = w0.cos();
    let a0_inv = 1.0 / (1.0 + alpha);
    let b0 = (1.0 - alpha) * a0_inv;
    let b1 = (-2.0 * cos_w0) * a0_inv;
    let b2 = (1.0 + alpha) * a0_inv;
    let a1 = b1;
    let a2 = b0;

    // Evaluate H(e^jω) at eval frequency
    let w = 2.0 * PI * freq_hz.clamp(1.0, sample_rate * 0.49) / sample_rate;
    let c1 = (-w).cos();
    let s1 = (-w).sin();
    let c2 = (-2.0 * w).cos();
    let s2 = (-2.0 * w).sin();
    let nr = b0 + b1 * c1 + b2 * c2;
    let ni = b1 * s1 + b2 * s2;
    let dr = 1.0 + a1 * c1 + a2 * c2;
    let di = a1 * s1 + a2 * s2;
    let denom = (dr * dr + di * di).max(1e-12);
    let hr = (nr * dr + ni * di) / denom;
    let hi = (ni * dr - nr * di) / denom;
    hi.atan2(hr)
}

fn unwrap_delta(mut d: f32) -> f32 {
    while d > PI {
        d -= 2.0 * PI;
    }
    while d < -PI {
        d += 2.0 * PI;
    }
    d
}

// ─── Math helpers ─────────────────────────────────────────────────────────────

pub fn oct_bandwidth_to_q(width_oct: f32) -> f32 {
    let bw = width_oct.clamp(0.03, 8.0);
    let p = 2.0_f32.powf(bw);
    (p.sqrt() / (p - 1.0).max(1e-6)).clamp(0.08, 240.0)
}

fn log_lerp(min: f32, max: f32, t: f32) -> f32 {
    10.0_f32.powf(min.log10() + (max.log10() - min.log10()) * t.clamp(0.0, 1.0))
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RuntimeSnapshot;

    #[test]
    fn bell_shape_peaks_at_center() {
        let peak = bell_shape(1000.0, 1000.0, 1.0);
        let side = bell_shape(2000.0, 1000.0, 1.0);
        assert!((peak - 1.0).abs() < 1e-6);
        assert!(side < 0.5);
    }

    #[test]
    fn low_shelf_is_monotone_decreasing() {
        // Low shelf should give more delay below cutoff
        let below = low_shelf_shape(200.0, 1000.0, 1.2);
        let above = low_shelf_shape(5000.0, 1000.0, 1.2);
        assert!(below > above, "below={below} above={above}");
    }

    #[test]
    fn high_shelf_is_monotone_increasing() {
        let below = high_shelf_shape(200.0, 1000.0, 1.1);
        let above = high_shelf_shape(5000.0, 1000.0, 1.1);
        assert!(above > below, "below={below} above={above}");
    }

    #[test]
    fn greedy_bell_peak_near_center() {
        let mut snap = RuntimeSnapshot::default();
        snap.max_sections = 64;
        snap.nodes[0] = NodeRuntimeParams {
            enabled: true,
            node_type: NodeType::Bell,
            freq_hz: 1000.0,
            amount_ms: 300.0,
            width_oct: 1.0,
            ..NodeRuntimeParams::default()
        };
        let result = run_greedy(&snap, 48_000.0);
        // Actual delay at 1 kHz > at 8 kHz
        let at_center: f32 = result.sections.iter().map(|s| match *s {
            SectionDescriptor::SecondOrder { freq_hz, q } =>
                group_delay_ms_one(1000.0, freq_hz, q, 48_000.0),
            _ => 0.0,
        }).sum::<f32>() + result.base_delay_ms;
        let at_far: f32 = result.sections.iter().map(|s| match *s {
            SectionDescriptor::SecondOrder { freq_hz, q } =>
                group_delay_ms_one(8000.0, freq_hz, q, 48_000.0),
            _ => 0.0,
        }).sum::<f32>() + result.base_delay_ms;
        assert!(at_center > at_far, "center={at_center:.2} far={at_far:.2}");
    }

    #[test]
    fn greedy_respects_max_sections() {
        let mut snap = RuntimeSnapshot::default();
        snap.max_sections = 16;
        snap.nodes[0] = NodeRuntimeParams {
            enabled: true,
            node_type: NodeType::Bell,
            freq_hz: 1000.0,
            amount_ms: 500.0,
            width_oct: 1.0,
            ..NodeRuntimeParams::default()
        };
        let result = run_greedy(&snap, 48_000.0);
        assert!(result.sections.len() <= 16);
    }

    #[test]
    fn scale_frequencies_a_minor_pentatonic_includes_440() {
        let node = NodeRuntimeParams {
            enabled: true,
            node_type: NodeType::Scale,
            scale_root: RootNote::A,
            scale_mode: ScaleMode::MinorPentatonic,
            ..NodeRuntimeParams::default()
        };
        let freqs = scale_frequencies(&node);
        // A4 = 440 Hz must be present
        assert!(
            freqs.iter().any(|&f| (f - 440.0).abs() < 0.5),
            "440 Hz not found in {:?}", &freqs[..freqs.len().min(10)]
        );
    }
}
