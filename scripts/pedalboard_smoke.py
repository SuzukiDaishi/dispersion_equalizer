from __future__ import annotations

import os
import platform
import shutil
from pathlib import Path

import numpy as np
import pedalboard

ROOT = Path(__file__).resolve().parents[1]
BUNDLE_DIR = ROOT / "target" / "bundled"
VST3_BUNDLE = BUNDLE_DIR / "Dispersion Equalizer.vst3"
AUV2_COMPONENT = BUNDLE_DIR / "Dispersion Equalizer.component"
AUV2_INSTALL_DIR = Path.home() / "Library" / "Audio" / "Plug-Ins" / "Components"
INSTALLED_AUV2_COMPONENT = AUV2_INSTALL_DIR / AUV2_COMPONENT.name
SAMPLE_RATE = 48_000


def resolve_plugin_candidates() -> list[Path]:
    system = platform.system()
    preferred = os.environ.get("PREFERRED_PLUGIN_FORMAT", "").casefold()

    if preferred:
        if system == "Darwin" and preferred in {"au", "auv2", "component"}:
            return resolve_auv2_candidates()
        if preferred in {"vst3", "vst"}:
            return resolve_vst3_candidates(system)
        raise ValueError(
            f"Unsupported PREFERRED_PLUGIN_FORMAT={preferred!r}; expected 'auv2' on macOS or 'vst3'."
        )

    candidates: list[Path] = [VST3_BUNDLE]
    candidates.extend(resolve_vst3_candidates(system))

    if system == "Darwin":
        candidates.extend(resolve_auv2_candidates())

    return unique_existing_paths(candidates)


def resolve_auv2_candidates() -> list[Path]:
    if AUV2_COMPONENT.exists():
        AUV2_INSTALL_DIR.mkdir(parents=True, exist_ok=True)
        if INSTALLED_AUV2_COMPONENT.exists():
            if INSTALLED_AUV2_COMPONENT.is_dir():
                shutil.rmtree(INSTALLED_AUV2_COMPONENT)
            else:
                INSTALLED_AUV2_COMPONENT.unlink()
        shutil.copytree(AUV2_COMPONENT, INSTALLED_AUV2_COMPONENT, symlinks=True)
        return [INSTALLED_AUV2_COMPONENT]

    return [INSTALLED_AUV2_COMPONENT] if INSTALLED_AUV2_COMPONENT.exists() else []


def resolve_vst3_candidates(system: str) -> list[Path]:
    candidates: list[Path] = []

    if system == "Windows":
        candidates.insert(
            0,
            VST3_BUNDLE / "Contents" / "x86_64-win" / "Dispersion Equalizer.vst3"
        )
    elif system == "Linux":
        candidates.insert(
            0,
            VST3_BUNDLE / "Contents" / "x86_64-linux" / "dispersion_equalizer.so"
        )
        candidates.extend((VST3_BUNDLE / "Contents").glob("*linux*/dispersion_equalizer.so"))
    elif system == "Darwin":
        candidates.append(VST3_BUNDLE / "Contents" / "MacOS" / "Dispersion Equalizer")

    return candidates


def unique_existing_paths(candidates: list[Path]) -> list[Path]:
    seen: set[Path] = set()
    existing: list[Path] = []
    for candidate in candidates:
        if candidate.exists() and candidate not in seen:
            seen.add(candidate)
            existing.append(candidate)
    return existing


def load_plugin_from_candidates() -> tuple[pedalboard.Plugin, Path]:
    candidates = resolve_plugin_candidates()
    if not candidates:
        raise FileNotFoundError(
            "No plugin bundle was found in target/bundled. Run "
            "`cargo xtask bundle dispersion_equalizer --release` first; on macOS "
            "run `cargo auv2 --release` to include AUv2. The AUv2 smoke test "
            "copies the component to ~/Library/Audio/Plug-Ins/Components before "
            "loading because Audio Units must be installed in a standard Components "
            "folder."
        )

    errors: list[str] = []
    for candidate in candidates:
        try:
            return pedalboard.load_plugin(str(candidate)), candidate
        except Exception as exc:  # noqa: BLE001
            errors.append(f"{candidate}: {exc}")

    joined = "\n".join(errors)
    raise RuntimeError(f"Failed to load plugin from all candidates:\n{joined}")


def warm_up(plugin: pedalboard.VST3Plugin, frames: int = 4096) -> None:
    silence = np.zeros((2, frames), dtype=np.float32)
    plugin.process(silence, SAMPLE_RATE, buffer_size=512, reset=True)


def make_audio(frames: int = 12_288) -> np.ndarray:
    rng = np.random.default_rng(20260529)
    noise = rng.standard_normal((2, frames)).astype(np.float32) * 0.03
    t = np.arange(frames, dtype=np.float32) / SAMPLE_RATE
    sine_l = 0.07 * np.sin(2.0 * np.pi * 220.0 * t)
    sine_r = 0.06 * np.sin(2.0 * np.pi * 880.0 * t + 0.5)
    return noise + np.stack([sine_l, sine_r]).astype(np.float32)


def main() -> int:
    plugin, plugin_path = load_plugin_from_candidates()
    print(f"[pedalboard-smoke] loading: {plugin_path}")

    assert plugin.name == "Dispersion Equalizer", f"Unexpected plugin name: {plugin.name}"
    assert plugin.is_effect, "Plugin is not exposed as an effect"
    assert "wet" in plugin.parameters, "Missing parameter: wet"
    assert "node_1_enabled" in plugin.parameters, "Missing parameter: node_1_enabled"
    assert "node_1_type" in plugin.parameters, "Missing parameter: node_1_type"

    valid_node_types = list(plugin.parameters["node_1_type"].valid_values)
    bell_type = "Bell" if "Bell" in valid_node_types else "Bell Delay"

    plugin.wet = 100.0
    plugin.node_1_enabled = True
    plugin.node_1_type = bell_type
    plugin.node_1_frequency_hz = 1400.0
    plugin.node_1_amount_ms = 120.0
    plugin.node_1_width_oct = 1.2

    assert float(plugin.wet) >= 99.0, "Failed to set wet parameter"
    assert bool(plugin.node_1_enabled), "Failed to set node_1_enabled parameter"

    audio = make_audio()

    baseline = pedalboard.load_plugin(str(plugin_path))
    baseline.wet = 0.0
    baseline.node_1_enabled = False
    warm_up(baseline)
    out_baseline = baseline.process(audio, SAMPLE_RATE, buffer_size=512, reset=False)

    shaped = pedalboard.load_plugin(str(plugin_path))
    shaped.wet = 100.0
    shaped.node_1_enabled = True
    shaped.node_1_type = bell_type
    shaped.node_1_frequency_hz = 1400.0
    shaped.node_1_amount_ms = 120.0
    shaped.node_1_width_oct = 1.2
    warm_up(shaped)
    out_shaped = shaped.process(audio, SAMPLE_RATE, buffer_size=512, reset=False)

    assert out_baseline.shape == audio.shape
    assert out_shaped.shape == audio.shape
    assert np.isfinite(out_baseline).all()
    assert np.isfinite(out_shaped).all()

    rms_delta = float(np.sqrt(np.mean((out_shaped - out_baseline) ** 2)))
    print(f"[pedalboard-smoke] rms_delta={rms_delta:.6f}")
    assert rms_delta > 1e-3, "Parameter changes did not create sufficient signal difference"

    print("[pedalboard-smoke] ok")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except AssertionError as exc:
        print(f"[pedalboard-smoke] assertion failed: {exc}")
        raise
    except Exception as exc:
        print(f"[pedalboard-smoke] failed: {exc}")
        raise
