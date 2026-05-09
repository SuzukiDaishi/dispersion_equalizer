from pathlib import Path

import numpy as np
import pedalboard
import pytest


ROOT = Path(__file__).resolve().parents[1]
VST3_PATH = (
    ROOT
    / "target"
    / "bundled"
    / "Dispersion Equalizer.vst3"
    / "Contents"
    / "x86_64-win"
    / "Dispersion Equalizer.vst3"
)
SAMPLE_RATE = 48_000


pytestmark = pytest.mark.skipif(
    not VST3_PATH.exists(),
    reason="VST3 bundle is missing; run `cargo xtask bundle dispersion_equalizer --release` first.",
)


def load_plugin(**params):
    plugin = pedalboard.load_plugin(str(VST3_PATH))
    for key, value in params.items():
        setattr(plugin, key, value)
    return plugin


def warm_up(plugin, frames=4096):
    silence = np.zeros((2, frames), dtype=np.float32)
    plugin.process(silence, SAMPLE_RATE, buffer_size=512, reset=True)


def test_vst3_loads_and_exposes_parameters():
    plugin = load_plugin()

    assert plugin.name == "Dispersion Equalizer"
    assert plugin.is_effect

    required = {
        "global_delay_ms",
        "wet",
        "output",
        "max_sos",
        "node_1_enabled",
        "node_1_type",
        "node_1_frequency_hz",
        "node_1_amount_ms",
        "node_1_width_oct",
        "node_1_root",
        "node_1_scale",
    }
    assert required.issubset(
        plugin.parameters.keys()
    ), f"Missing: {required - set(plugin.parameters.keys())}"

    valid_types = plugin.parameters["node_1_type"].valid_values
    assert "Bell Delay" in valid_types
    assert "Low Shelf" in valid_types
    assert "High Shelf" in valid_types
    assert "Scale / Pentatonic" in valid_types
    assert "Disperser" not in valid_types


def test_default_patch_is_finite_passthrough():
    plugin = load_plugin()
    t = np.arange(4096, dtype=np.float32) / SAMPLE_RATE
    left = 0.15 * np.sin(2.0 * np.pi * 440.0 * t)
    right = 0.12 * np.sin(2.0 * np.pi * 997.0 * t + 0.25)
    audio = np.stack([left, right]).astype(np.float32)

    output = plugin.process(audio, SAMPLE_RATE, buffer_size=512, reset=True)

    assert output.shape == audio.shape
    assert output.dtype == np.float32
    assert np.isfinite(output).all()
    np.testing.assert_allclose(output, audio, rtol=0.0, atol=1e-6)


def test_global_delay_moves_impulse_after_warmup():
    delay_ms = 12.0
    plugin = load_plugin(global_delay_ms=delay_ms, wet=100.0)
    warm_up(plugin)

    impulse = np.zeros((2, 5000), dtype=np.float32)
    impulse[:, 0] = 1.0
    output = plugin.process(impulse, SAMPLE_RATE, buffer_size=512, reset=False)

    expected_peak = round(SAMPLE_RATE * delay_ms / 1000.0)
    peak = int(np.argmax(np.abs(output[0])))

    assert np.isfinite(output).all()
    assert peak == expected_peak
    assert np.max(np.abs(output[:, : expected_peak - 1])) < 1e-6
    np.testing.assert_allclose(output[:, expected_peak], [1.0, 1.0], rtol=0.0, atol=1e-6)


@pytest.mark.parametrize(
    "node_type,extra_params",
    [
        ("Bell Delay", {"node_1_width_oct": 1.2}),
        ("Low Shelf", {"node_1_frequency_hz": 300.0}),
        ("High Shelf", {"node_1_frequency_hz": 4000.0}),
    ],
)
def test_node_types_are_finite_and_change_signal(node_type, extra_params):
    params = {
        "node_1_enabled": True,
        "node_1_type": node_type,
        "node_1_frequency_hz": 1200.0,
        "node_1_amount_ms": 80.0,
        "wet": 100.0,
        **extra_params,
    }
    plugin = load_plugin(**params)
    warm_up(plugin)

    rng = np.random.default_rng(7)
    audio = (rng.standard_normal((2, 16_384)).astype(np.float32) * 0.05).astype(np.float32)
    output = plugin.process(audio, SAMPLE_RATE, buffer_size=512, reset=False)

    assert output.shape == audio.shape
    assert np.isfinite(output).all()
    assert np.max(np.abs(output)) < 1.0
    rms_delta = float(np.sqrt(np.mean((output - audio) ** 2)))
    assert rms_delta > 1e-3, f"Signal unchanged for {node_type}: rms_delta={rms_delta}"


def test_max_sos_limits_sections():
    """Lower max_sos should produce less-accurate but still finite output."""
    params = {
        "node_1_enabled": True,
        "node_1_type": "Bell Delay",
        "node_1_frequency_hz": 1000.0,
        "node_1_amount_ms": 200.0,
        "wet": 100.0,
        "max_sos": 8,
    }
    plugin = load_plugin(**params)
    warm_up(plugin)

    rng = np.random.default_rng(42)
    audio = (rng.standard_normal((2, 8192)).astype(np.float32) * 0.05).astype(np.float32)
    output = plugin.process(audio, SAMPLE_RATE, buffer_size=512, reset=False)

    assert np.isfinite(output).all()
    assert np.max(np.abs(output)) < 1.0
