# Vendored GUI crates

This directory contains locally patched copies of the plugin's GUI stack. They are
vendored because the upstream crates pin an old revision of `baseview` that has known
DPI-scaling and mouse-input bugs on some environments (scaled Windows setups, remote
desktop, Wine/yabridge, X11 with non-standard scaling), and neither upstream repo has
picked up the fixed `baseview` yet.

| Crate | Vendored from | Upstream revision |
| --- | --- | --- |
| `egui-baseview` | <https://github.com/BillyDM/egui-baseview> (archived) | `ec70c3fe6b2f070dcacbc22924431edbe24bd1c0` |
| `nih_plug_egui` | <https://github.com/robbert-vdh/nih-plug> | `f36931f7af4646065488a9845d8f8c2f95252c23` |

Licenses are preserved next to each crate (`egui-baseview`: MIT, `nih_plug_egui`: ISC).

## Local changes on top of the upstream revisions

- **`baseview` updated to `RustAudio/baseview` `a084fc1a` (v0.2.1, 2026-07-13)** and both
  crates ported to its new API (`WindowHandler` by `&self` + `WindowContext`,
  raw-window-handle 0.6, keyboard-types 0.8, `dpi` size types, physical-pixel mouse
  positions). This picks up the upstream fixes for:
  - Win32 Per-Monitor V2 DPI awareness and window-creation DPI (wrong editor size /
    mouse clicks landing at wrong coordinates on scaled Windows setups)
  - X11 scaling fallback
  - Wine child-window GL pixel format selection (yabridge)
- **Relaxed OpenGL requirements** in `nih_plug_egui`: no depth/stencil buffer and no
  sRGB-capable framebuffer. `egui_glow` disables `FRAMEBUFFER_SRGB` and outputs
  gamma-space colors, so the sRGB config was unused — but requiring it made GL context
  creation fail outright on VMs, remote desktops, and other environments without sRGB
  framebuffer configs.
- **egui zoom-factor aware input handling** in `egui-baseview`, plus a public
  `EguiState::set_requested_size`, used by the plugin's UI scale setting.
- The unused `wgpu` renderer was dropped from the vendored `egui-baseview`.

## When to remove this directory

If nih-plug upstream moves `nih_plug_egui`/`egui-baseview` to `baseview` >= 0.2, the
vendored copies can be dropped and `Cargo.toml` pointed back at the upstream git
dependencies.
