# SatComp

A waveshaping saturator with a compressor and a 2-stage 3-band EQ, in a black-and-white monospace UI. Made with bricking intentions.

![SatComp](docs/screenshot.png)

VST3 and CLAP for macOS (Apple silicon and Intel) and Windows. Current version: **0.5.2**.

## Features
- **Saturator:** 15 shapers including a **Custom** one you draw yourself (pencil icon, up to 10 points), drive, bias, HP/LP split, tone, sat mix, output gain, 1x/2x/4x oversampling.
- **Sidechain:** external key from a second stereo input, with gain, HP and LP on the key and a SOLO to listen to it.
- **Compressor:** threshold, ratio, attack, release, knee and knee shape, peak/RMS detector, low cut, lookahead, feed-forward/feedback blend, GR distortion, makeup and mix.
- **EQ:** two stages (pre and post compressor), three bands each plus tilt, with a live spectrum.
- **Metering and tools:** output meter, gain-reduction meter, GAIN MATCH, DELTA monitor, BYPASS.
- **Waveform scope:** click the icon in the corner of the compressor plot for a half or full wave view of the output with the gain reduction stacked on top (transient meter). Drag it to change the time window and the ceiling (+2 dB to -14 dB, so inter-sample overshoots stay visible).
- **Presets:** save and load your own, step through them, six Drums factory presets included. A name wrapped in asterisks (`*NAME*`) means the preset has been modified.
- **20 colour schemes, each with a light mode, plus your own:** click the icon for the next scheme, double-click it for light/dark, right-click for the list. The **Appearance** tab (About page) has the mode switch, a scheme list with swatches, six fonts and a theme editor with a colour picker; user themes are saved in `~/Library/Application Support/SatComp/themes.json` (`%APPDATA%\SatComp\themes.json` on Windows).
- Resizable window. Double-click any slider to reset it.

Click the title for an About page with the signal flow, the Appearance tab and the changelog.

## Install
Download the zip for your system from the [Releases](../../releases) page and copy the bundles:

| | macOS | Windows |
|---|---|---|
| VST3 | `~/Library/Audio/Plug-Ins/VST3/` | `C:\Program Files\Common Files\VST3\` |
| CLAP | `~/Library/Audio/Plug-Ins/CLAP/` | `C:\Program Files\Common Files\CLAP\` |

Then rescan plug-ins in your DAW.

**macOS:** the build is not notarized. If Gatekeeper blocks it, run `xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/satcomp.vst3` (and the same for the CLAP bundle).

User presets are saved to `~/Library/Application Support/SatComp/Presets` (macOS) or `%APPDATA%\SatComp\Presets` (Windows).

## Build
Requires [Rust](https://rustup.rs).

```bash
cd rust
cargo test
cargo xtask bundle satcomp --release            # this machine's architecture
cargo xtask bundle-universal satcomp --release  # macOS: arm64 + x86_64
```

Bundles land in `rust/target/bundled/`. GitHub Actions builds the macOS universal and Windows bundles on every push (see `.github/workflows/build.yml`) and attaches them to a release when a `v*` tag is pushed.

## Repository
- `rust/`: the production plug-in (`satcomp-dsp` engine built from reusable blocks, `satcomp-ui` shared egui toolkit, `satcomp-plugin` nih-plug shell and the SatComp pages, factory presets in `satcomp-plugin/presets/`).
- `SatComp.jsfx`: the original REAPER JSFX prototype. Archival, frozen at v0.3.
- `docs/`: `FOUNDATION.md` (workflow and gotchas), `PARAMETERS.md` (parameter reference), `HANDOFF.md` (JSFX-era design decisions), `TEST_CHECKLIST.md`.

## Credits and license
GPL-3.0-or-later, see `LICENSE`. Built with [nih-plug](https://github.com/robbert-vdh/nih-plug) and [egui](https://github.com/emilk/egui). Typefaces (all SIL OFL 1.1, embedded): [Fira Code](https://github.com/tonsky/FiraCode), JetBrains Mono, Geist Mono, Roboto Mono, Oxygen Mono and Doto; licences are in `Fonts/`.
