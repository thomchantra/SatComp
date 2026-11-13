# SatComp

Three saturator-based dynamics plug-ins in one black-and-white (or any colour you like) monospace UI. Made with bricking intentions.

| Plug-in | Version | What |
|---|---|---|
| [**SatComp**](#satcomp) | 0.5.2 | waveshaping saturator, compressor and a 2-stage 3-band EQ |
| [**SatXComp**](#satxcomp) | 0.1.0 | 4-band multiband compressor with a saturator per band and a 5-band EQ |
| [**SatLim**](#satlim) | 0.1.0 | saturator into an oversampled clipper and a true-peak brickwall limiter |

VST3 and CLAP for macOS (Apple silicon and Intel) and Windows. Free, GPL-3.0-or-later.

## SatComp

A waveshaping saturator with a compressor and a 2-stage 3-band EQ. Give the compressor more spice to squash.

![SatComp](docs/screenshot.png)

- **Saturator:** 15 shapers including a **Custom** one you draw yourself (pencil icon), drive, bias, ASYM (how much of the even-order part each shaper keeps, so lopsided curves do not drag DC into the signal), HP/LP split, tone, sat mix, SAT GAIN with auto-match, 1x/2x/4x oversampling.
- **Sidechain:** external key from a second stereo input, with gain, HP and LP on the key and a SOLO to listen to it.
- **Compressor:** threshold, ratio, attack, release, knee and knee shape, peak/RMS detector, low cut, lookahead, feed-forward/feedback blend, GR distortion, makeup and mix.
- **EQ:** two stages (pre and post compressor), three bands each plus tilt, with a live spectrum.
- **Output limiter:** a true-peak brickwall at -0.2 dBTP, on by default (LIM), with about 2 ms of latency that is always reported to the host.
- **Metering and tools:** output meter, gain-reduction meter, post-saturator level meter, GAIN MATCH, DELTA monitor, plain BYPASS.
- **Waveform scope:** click the icon in the corner of the compressor plot for a half or full wave view of the output with the gain reduction stacked on top (transient meter).
- **Presets:** save and load your own, step through them, six Drums factory presets included. A name wrapped in asterisks (`*NAME*`) means the preset has been modified.

## SatXComp

A 4-band compressor with a waveshaping saturator on every band and a 5-band EQ. For surgical baking: split the signal, then drive each band to taste.

![SatXComp](docs/screenshot-satxcomp.png)

- **Four bands** (LO, ML, MH, HI) split by three Linkwitz-Riley crossovers, each with a 12, 24, 48 or 96 dB slope. Drag the crossover lines in the spectrum or use the XOV sliders. The bands sum flat.
- **Per band:** a saturator (same 15 shapers, custom curves with a symmetry tool, band-limited so it only adds harmonics inside the band), a compressor, SOLO and DELTA, and its own SAT GAIN auto-match.
- **Compressor modes per band:** DWNWARD (the plain compressor), UPWARD (lifts what is below the threshold, up to RANGE) or UP/DWN (both).
- **Sidechain:** the external key is split across the bands, so it ducks like a dynamic EQ; KEY MIX blends it with the band's own level.
- **Spectrum viewport:** the output over the dry signal. The accent colour shows where the compressor took gain away, its inverse where gain was added. Each band handle sits at the threshold with a bar for the gain reduction below it, the gain increase above it, and the in/out difference on top.
- **EQ:** a 5-band post EQ with notch, bandpass and allpass types, tilt, and a mix control for blending with the dry signal.
- **Output limiter** (true-peak, on by default), waveform scope, delta, plain bypass and four factory presets.

## SatLim

A 2-stage clipper/limiter with a waveshaping saturator. When you need to nuke it from orbit.

![SatLim](docs/screenshot-satlim.png)

- **Chain:** saturator (x1 to x8 oversampling) into a hard **CLIP** at the ceiling, then a true-peak brickwall limiter, then OUT GAIN. The output gain is after the limiter on purpose.
- **THRESHOLD** is the input level that lands on the ceiling: lowering it chews the signal deeper. **CLIP** exposes and bricks more transients, **RELEASE** trades density against distortion.
- **L.AHEAD** on gives a lookahead true-peak limiter; off gives a zero-latency one that follows sample peaks. The **TRU.PK** readout is an 8x true peak either way.
- **Transient scope** shows the output with what the limiter removed (solid) and what the clipper removed (hatched) stacked on top; the GR meter splits the same way.
- Auto-matched SAT GAIN, DELTA (what the clipper and limiter removed), plain BYPASS and six factory presets.

## Shared

- **27 colour schemes, each with a light mode, plus your own:** click the logo for the next scheme, double-click it for light/dark, right-click for the list. The **Preferences** tab (About page) has the mode switch, a scheme list with swatches, 11 fonts, a custom theme editor with a colour picker, and SET AS DEFAULT for new instances. User themes are saved in `~/Library/Application Support/SatComp/themes.json` (`%APPDATA%\SatComp\themes.json` on Windows).
- Resizable window (100% to 250%). Double-click any slider to reset it. Click the title for an About page with the signal flow, Preferences and the changelog.

## Install
Download the zip for your plug-in and system from the [Releases](../../releases) page and copy the bundles (`satcomp`, `satxcomp` or `satlim`):

| | macOS | Windows |
|---|---|---|
| VST3 | `~/Library/Audio/Plug-Ins/VST3/` | `C:\Program Files\Common Files\VST3\` |
| CLAP | `~/Library/Audio/Plug-Ins/CLAP/` | `C:\Program Files\Common Files\CLAP\` |

Then rescan plug-ins in your DAW.

**macOS:** the builds are not notarized. If Gatekeeper blocks one, run `xattr -dr com.apple.quarantine ~/Library/Audio/Plug-Ins/VST3/satcomp.vst3` (and the same for the other bundles and the CLAP ones).

User presets are saved to `~/Library/Application Support/<SatComp|SatXComp|SatLim>/Presets` (macOS) or `%APPDATA%\<name>\Presets` (Windows).

## Build
Requires [Rust](https://rustup.rs).

```bash
cd rust
cargo test
cargo xtask bundle satcomp --release            # or satxcomp, satlim; this machine's architecture
cargo xtask bundle-universal satcomp --release  # macOS: arm64 + x86_64
```

Bundles land in `rust/target/bundled/`. GitHub Actions builds the macOS universal and Windows bundles on every push (see `.github/workflows/build.yml`). A `v*` tag releases all the plug-ins together; `satxcomp-v*` and `satlim-v*` tags release one plug-in on its own.

## Repository
- `rust/`: the production plug-ins. Shared crates: `satcomp-dsp` (saturator, compressor, EQ and limiter blocks) and `satcomp-ui` (egui toolkit, colour schemes, fonts). One DSP crate plus one nih-plug shell per plug-in (`satcomp-*`, `satxcomp-*`, `satlim-*`); factory presets are in each `*-plugin/presets/`.
- `SatComp.jsfx`: the original REAPER JSFX prototype. Archival, frozen at v0.3.
- `docs/`: `FOUNDATION.md` (workflow and gotchas), `INVENTORY.md` (versions and milestones), `PARAMETERS.md` (parameter reference), `SATLIM.md`, `HANDOFF.md` (JSFX-era design decisions), `TEST_CHECKLIST.md`, and the release notes in `docs/releases/`.

## Credits and license
GPL-3.0-or-later, see `LICENSE`. Built with [nih-plug](https://github.com/robbert-vdh/nih-plug) and [egui](https://github.com/emilk/egui). Typefaces (all SIL OFL 1.1, embedded): [Fira Code](https://github.com/tonsky/FiraCode), JetBrains Mono, Geist Mono, Roboto Mono, Oxygen Mono, Doto, DM Mono, Victor Mono, Anonymous Pro, Inconsolata and Courier Prime; licences are in `Fonts/`.
