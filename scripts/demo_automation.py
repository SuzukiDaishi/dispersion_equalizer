"""
Dispersion Equalizer -- dynamic automation demo
Processes sample/dnb_sine_noise_174bpm.wav with live parameter animation
and saves the result to test_artifacts/demo_output.wav.

174 BPM  -> bar = 1.379 s
Timeline (16 bars, ~22 s):
  0- 2 bars ( 0.0- 2.8s): dry intro, wet fading in
  2- 5 bars ( 2.8- 6.9s): Bell sweep 250Hz->6kHz, amount=80ms
  5- 8 bars ( 6.9-11.0s): Bell LFO 300<->3000Hz at 1.5Hz, amount ramp 30->120ms
  8-12 bars (11.0-16.6s): Bell steady 1kHz, HighShelf joins sweeping 2->10kHz
 12-16 bars (16.6-22.1s): Bell fast LFO + LowShelf modulated + HighShelf oscillates
"""

import sys
from pathlib import Path

import numpy as np
import pedalboard
from pedalboard.io import AudioFile

ROOT = Path(__file__).resolve().parents[1]
VST3 = (
    ROOT
    / "target/bundled/Dispersion Equalizer.vst3"
    / "Contents/x86_64-win"
    / "Dispersion Equalizer.vst3"
)
INPUT = ROOT / "sample/dnb_sine_noise_174bpm.wav"
OUTPUT = ROOT / "test_artifacts/demo_output.wav"

SAMPLE_RATE = 48_000
CHUNK = 512  # ~11 ms -- smaller than 30ms debounce so debounce fires correctly


def log_interp(a: float, b: float, t: np.ndarray) -> np.ndarray:
    t = np.clip(t, 0.0, 1.0)
    return np.exp(np.log(a) + (np.log(b) - np.log(a)) * t)


def sine_lfo(t_arr: np.ndarray, rate_hz: float, lo: float, hi: float, log: bool = True) -> np.ndarray:
    phase = (np.sin(2.0 * np.pi * rate_hz * t_arr) + 1.0) * 0.5  # 0..1
    if log:
        return log_interp(lo, hi, phase)
    return lo + phase * (hi - lo)


def build_automation(t: np.ndarray) -> dict:
    n = len(t)

    s0 = t < 2.76             # bars 1-2: intro
    s1 = (t >= 2.76) & (t < 6.9)  # bars 2-5: Bell sweep
    s2 = (t >= 6.9)  & (t < 11.0) # bars 5-8: Bell LFO + amount ramp
    s3 = (t >= 11.0) & (t < 16.6) # bars 8-12: steady Bell + HighShelf
    s4 = t >= 16.6              # bars 12-16: full modulation

    # Wet: fade 0->80% over 2.76s, hold at 80%
    wet = np.where(t < 2.76, t / 2.76 * 80.0, 80.0)

    # --- Node 1: Bell ---
    n1_freq = np.full(n, 1000.0)
    n1_freq[s1] = log_interp(250.0, 6000.0, (t[s1] - 2.76) / (6.9 - 2.76))
    n1_freq[s2] = sine_lfo(t[s2] - 6.9, 1.5, 300.0, 3000.0, log=True)
    n1_freq[s3] = 1000.0
    n1_freq[s4] = sine_lfo(t[s4] - 16.6, 3.0, 200.0, 4000.0, log=True)

    n1_amount = np.full(n, 50.0)
    n1_amount[s0] = 0.0
    n1_amount[s1] = 80.0
    n1_amount[s2] = 30.0 + (t[s2] - 6.9) / (11.0 - 6.9) * 90.0   # 30->120ms
    n1_amount[s3] = 100.0
    n1_amount[s4] = sine_lfo(t[s4] - 16.6, 2.0, 40.0, 130.0, log=False)

    # --- Node 2: HighShelf (active bars 8-16) ---
    n2_freq = np.full(n, 4000.0)
    n2_freq[s3] = log_interp(2000.0, 10000.0, (t[s3] - 11.0) / 5.6)
    n2_freq[s4] = sine_lfo(t[s4] - 16.6, 0.7, 1500.0, 12000.0, log=True)

    n2_amount = np.full(n, 60.0)
    n2_amount[s4] = sine_lfo(t[s4] - 16.6, 0.9, 30.0, 100.0, log=False)

    # --- Node 3: LowShelf (active bars 12-16 only) ---
    n3_freq = np.full(n, 200.0)
    n3_amount = sine_lfo(t, 1.3, 20.0, 80.0, log=False)

    return dict(
        wet=wet,
        n1_freq=n1_freq,
        n1_amount=n1_amount,
        n2_freq=n2_freq,
        n2_amount=n2_amount,
        n3_freq=n3_freq,
        n3_amount=n3_amount,
    )


def main() -> None:
    for path, label in [(VST3, "VST3"), (INPUT, "input WAV")]:
        if not path.exists():
            sys.exit(f"{label} not found: {path}")

    print(f"Loading {INPUT.name}...")
    with AudioFile(str(INPUT)) as f:
        audio = f.read(f.frames)
        sr = f.samplerate

    if sr != SAMPLE_RATE:
        sys.exit(f"Expected {SAMPLE_RATE} Hz, got {sr}")

    n_frames = audio.shape[1]
    dur = n_frames / SAMPLE_RATE
    t = np.linspace(0.0, dur, n_frames, endpoint=False)
    print(f"  {dur:.2f}s  {audio.shape[0]}ch  max_input={np.max(np.abs(audio)):.3f}")

    auto = build_automation(t)

    print(f"\nLoading plugin...")
    plugin = pedalboard.load_plugin(str(VST3))

    # All nodes are configured from the start to avoid section-count jumps mid-stream.
    plugin.wet = 0.0
    plugin.global_delay_ms = 0.0
    plugin.output_db = -3.0   # headroom for allpass constructive peaks

    plugin.node_1_enabled = True
    plugin.node_1_type = "Bell Delay"
    plugin.node_1_frequency_hz = 1000.0
    plugin.node_1_amount_ms = 0.0
    plugin.node_1_width_oct = 1.5

    plugin.node_2_enabled = True       # enabled from start, amount=60ms initially
    plugin.node_2_type = "High Shelf"
    plugin.node_2_frequency_hz = 4000.0
    plugin.node_2_amount_ms = 60.0
    plugin.node_2_width_oct = 2.0

    plugin.node_3_enabled = True       # enabled from start, amount=20ms initially
    plugin.node_3_type = "Low Shelf"
    plugin.node_3_frequency_hz = 200.0
    plugin.node_3_amount_ms = 20.0
    plugin.node_3_width_oct = 1.5

    # Process in chunks with per-chunk parameter updates
    n_chunks = (n_frames + CHUNK - 1) // CHUNK
    out_chunks: list[np.ndarray] = []

    timeline = [
        (0.0,  "  0-2 bars:  dry intro, wet fading in"),
        (2.76, "  2-5 bars:  Bell sweep 250Hz->6kHz"),
        (6.9,  "  5-8 bars:  Bell LFO, amount ramp 30->120ms"),
        (11.0, "  8-12 bars: HighShelf joins, Bell@1kHz"),
        (16.6, " 12-16 bars: all nodes, fast modulation"),
    ]
    label_idx = 0

    print(f"\nProcessing {n_chunks} chunks x {CHUNK} samples (~{CHUNK/SAMPLE_RATE*1000:.0f} ms each)...\n")

    for i in range(n_chunks):
        s = i * CHUNK
        e = min(s + CHUNK, n_frames)
        idx = (s + e) // 2
        tc = idx / SAMPLE_RATE

        while label_idx < len(timeline) and tc >= timeline[label_idx][0]:
            print(timeline[label_idx][1])
            label_idx += 1

        plugin.wet = float(auto["wet"][idx])
        plugin.node_1_frequency_hz = float(np.clip(auto["n1_freq"][idx], 20.0, 20000.0))
        plugin.node_1_amount_ms    = float(np.clip(auto["n1_amount"][idx], 0.0, 1000.0))
        plugin.node_2_frequency_hz = float(np.clip(auto["n2_freq"][idx], 20.0, 20000.0))
        plugin.node_2_amount_ms    = float(np.clip(auto["n2_amount"][idx], 0.0, 1000.0))
        plugin.node_3_frequency_hz = float(np.clip(auto["n3_freq"][idx], 20.0, 20000.0))
        plugin.node_3_amount_ms    = float(np.clip(auto["n3_amount"][idx], 0.0, 1000.0))

        chunk_out = plugin.process(
            audio[:, s:e], SAMPLE_RATE, buffer_size=CHUNK, reset=(i == 0)
        )
        out_chunks.append(chunk_out)

    output = np.concatenate(out_chunks, axis=1)
    peak = float(np.max(np.abs(output)))
    print(f"\nPeak before normalization: {peak:.4f}")

    if peak > 0.98:
        output = output / peak * 0.97
        print(f"Normalized to 0.97")

    OUTPUT.parent.mkdir(exist_ok=True)
    with AudioFile(str(OUTPUT), "w", SAMPLE_RATE, num_channels=output.shape[0]) as f:
        f.write(output)

    print(f"\nSaved -> {OUTPUT.name}")
    print()
    print("Sections to listen for:")
    for ts, label in timeline:
        print(f"  {ts:5.1f}s {label.strip()}")


if __name__ == "__main__":
    main()
