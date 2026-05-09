# Dispersion Equalizer

Dispersion Equalizer is a group-delay EQ built with Rust and NIH-plug. Instead of changing gain per frequency band, it changes arrival time per frequency band with pure delay and all-pass filters while keeping the wet path as amplitude-flat as practical.

Current MVP features:

- CLAP/VST3 build targets through NIH-plug
- Global Delay
- Bell Delay nodes
- Disperser nodes
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

For development checks:

```shell
cargo check
cargo test
```

The MVP packaging target is CLAP. VST3 export is kept buildable, but distribution policy should be reviewed before publishing VST3 builds because of the VST3 binding license considerations documented in `docs/pre_spec.md` and `plan.md`.
