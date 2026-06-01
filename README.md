# Dispersion Equalizer

![Dispersion Equalizer UI](docs/view.png)

Dispersion Equalizer is a group-delay EQ built with Rust and NIH-plug. Instead of changing gain per frequency band, it changes arrival time per frequency band with pure delay and all-pass filters while keeping the wet path as amplitude-flat as practical.

Current MVP features:

- CLAP/VST3 build targets through NIH-plug
- Global Delay
- Bell Delay nodes
- Bell/Shelf/Scale delay nodes
- Fixed 16 node slots for DAW automation
- Dark egui graph editor with target/actual delay curves
- Node add/select/drag/duplicate/remove
- Built-in starter presets
- DSP unit tests for delay line, all-pass stability, compiler behavior, and graph mapping

The plugin is designed around 100% wet use. Lower wet values intentionally blend dry and phase-shifted wet audio, so comb filtering can occur.

## Building

After installing [Rust](https://rustup.rs/), you can compile Dispersion Equalizer as follows:

```shell
cargo xtask bundle dispersion_equalizer --release
```

On macOS, you can build release CLAP/VST3 + AUv2 bundles with one command:

```shell
cargo auv2 --release
```

`cargo auv2` also defaults to release output so the AUv2 arm64 slice matches the
release x86_64 slice used for universal macOS packages. Use `cargo xtask auv2
--debug` only for local debugging.

For development checks:

```shell
cargo check
cargo test
uv sync --frozen
uv run python scripts/pedalboard_smoke.py
```

If `uv sync --frozen` fails in CI after dependency changes, regenerate the lockfile:

```shell
uv lock
```

### Quick commands

- All platforms (CLAP/VST3): `cargo xtask bundle dispersion_equalizer --release`
- macOS only (CLAP/VST3/AUv2): `cargo auv2 --release`
- Pedalboard smoke test: `uv run python scripts/pedalboard_smoke.py`

Release instructions are documented in `docs/release.md`.

CLAP, VST3, and AUv2 (Audio Unit) are shipped in every release. macOS builds are Universal Binary (arm64 + x86_64), Developer ID signed, and notarized.

## Audio Unit (AUv2) on macOS

The macOS DMG includes an AUv2 `.component` for Logic Pro and GarageBand.

**Install:**

```bash
cp -R "Dispersion Equalizer.component" ~/Library/Audio/Plug-Ins/Components/
```

**If macOS blocks the plugin (unsigned/quarantine):**

```bash
xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/Components/"Dispersion Equalizer.component"
```

**Validate:**

```bash
auval -strict -v aufx DsEQ Zuky
```

Then rescan Audio Units in your DAW or restart it.

## Pedalboard smoke test

After bundling the plugin, run a minimal plugin smoke test with Python Pedalboard:

```shell
uv sync --frozen
uv run python scripts/pedalboard_smoke.py
```

On macOS, force the smoke test to load only the AUv2 component instead of
falling back to VST3 when checking Logic/GarageBand-facing output. The script
copies `target/bundled/Dispersion Equalizer.component` to
`~/Library/Audio/Plug-Ins/Components/` first because Audio Units must be installed
in a standard Components folder before Pedalboard/macOS can scan them:

```shell
PREFERRED_PLUGIN_FORMAT=auv2 uv run python scripts/pedalboard_smoke.py
```

This smoke test checks that the plugin can be loaded, parameters can be set, and
parameter changes create a measurable output difference.
