# Inventory

Internal record of what exists, at which version, and when it got there. Update it with every release (the checklist is at the bottom). Dates are commit dates; tags are on `main`.

## Plug-ins

| | SatComp | SatXComp | SatLim |
|---|---|---|---|
| Version | 0.5.2 | 0.1.0 | 0.1.0 |
| Crates | `satcomp-dsp` (shared blocks), `satcomp-plugin` | `satxcomp-dsp`, `satxcomp-plugin` | `satlim-dsp`, `satlim-plugin` |
| Chain | saturator, compressor, two 3-band EQ stages (pre / post compressor) + tilt, sidechain, output limiter | 4-band split > per band sat > comp > comp mix/gain, 5-band post EQ, limiter, sidechain | sat (x1-x8) > clip > true-peak limiter > out gain |
| VST3 class id | `TrtSatCompPlugin` | `thomchantraSatXC` | `thomchantraSatLm` |
| CLAP id | `com.trirat.satcomp` | `com.thomchantra.satxcomp` | `com.thomchantra.satlim` |
| Bundle names | `satcomp.vst3` / `.clap` | `satxcomp.vst3` / `.clap` | `satlim.vst3` / `.clap` |
| Preset extension / folder | `.satcomp`, `SatComp/Presets` | `.satxcomp`, `SatXComp/Presets` | `.satlim`, `SatLim/Presets` |
| Factory presets | 6 (Drums_*) | 4 | 6 |
| Window (logical, base) | 560 x 336 | 664 x 400 | 340 x 496 |
| Latency | constant ~2.2 ms at 48 kHz, reported | constant, reported | saturator + clipper oversamplers + lookahead, reported; changes with x1-x8 and L.AHEAD |
| Docs | `PARAMETERS.md` (JSFX era), `FOUNDATION.md` | `FOUNDATION.md` | `SATLIM.md` |

Shared: `satcomp-ui` (canvas, 27 palettes, 11 embedded fonts, FFT, widget kit, About / Preferences / Changelog pages), `themes.json` and `defaults*.json` under `SatComp/` in the user's application-support folder. Parameter ids never change once released (presets, projects and automation depend on them).

**Not built:** SatEQ (5-band EQ with a waveshaper per band) exists only as a pitch, `docs/internal/SATEQ_PITCH.md`. The CI matrix knows the `sateq-v*` tag but there is no crate. AU is shelved (no demand yet).

## Release and tag convention
- `v*` tag: bundle release of everything that changed (0.5.2 is the first bundle). `satxcomp-v*`, `satlim-v*`, `sateq-v*`: a single plug-in on its own.
- Zip name = `<Display>-v<the plug-in's own Cargo.toml version>-<platform>.zip`, not the tag's version.
- Manuals: `docs/manual/<Plugin>.md` is the source, `python3 docs/manual/build.py` writes `<Plugin>-Manual-v<version>.pdf` (committed; CI attaches it to the release and puts it in the zips). Rebuild it whenever the plug-in version changes.
- Zip layout: `VST3/<plugin>.vst3`, `CLAP/<plugin>.clap`, `INSTALL.txt` (from `docs/install/`), `LICENSE`, the manual.
- Notes file `docs/releases/<tag>.md` before tagging (the release workflow uses it as the body); verify the release through the API afterwards. See `FOUNDATION.md`.

## Version history

### SatComp
| Version | Date | Milestone |
|---|---|---|
| JSFX v0.1 | 2026-11-08 | first prototype: saturator + compressor + gain diff, custom monospace UI (`SatComp.jsfx`) |
| JSFX v0.2 | 2026-11-08/09 | grid UI, bypass, meters, delta monitor, Sat Mix, Sat Out, 14 shapers |
| JSFX v0.3 | 2026-11-09 | two 3-band EQ stages + tilt, lookahead, GAIN MATCH. Last JSFX; archival |
| v0.4 / v0.4.1 | 2026-11-09 | rebuilt in Rust as VST3 + CLAP, presets, resize, custom waveshaper, 20 schemes; CI for macOS universal + Windows; v0.4.1 fixes the PK readout |
| v0.5.0 | 2026-11-09 | sidechain input, light mode for every scheme, scrolling About |
| v0.5.1 | 2026-11-10 | fixed the enable/refocus thump; Appearance tab, custom themes, waveform scope |
| v0.5.2 | 2026-11-13 | engine rebuilt from blocks (bit-identical audio), shared UI crate, true-peak output limiter, post-sat meter, ASYM, SAT GAIN, plain bypass, 27 schemes, 11 fonts, per-plug-in defaults |

### SatXComp
| Version | Date | Milestone |
|---|---|---|
| (design) | 2026-11-10 | started as a 3-band multiband version of SatComp; `satxcomp-dsp` crossover |
| 0.1.0 | 2026-11-13 | frozen with 0.5.2: four bands, 12/24/48/96 dB crossovers, band-limited saturator, DWNWARD / UPWARD / UP/DWN compressor, per-band sidechain key, 5-band EQ, limiter, scope, gain-reduction and gain-increase view in the spectrum, 4 factory presets |

### SatLim
| Version | Date | Milestone |
|---|---|---|
| 0.1.0 | 2026-11-13 | frozen with 0.5.2: the SatComp saturator into an oversampled clipper and true-peak limiter with transient scope, 8x TRU.PK readout, 6 factory presets |

## Key milestones (project timeline)
| Date | Event |
|---|---|
| 2026-11-08 | repository starts with the SatComp JSFX prototype |
| 2026-11-09 | Rust rebuild (v0.4), first GitHub releases v0.4.1 and v0.5.0 |
| 2026-11-10 | v0.5.1 released; the two regression nets recorded (101-scenario audio golden, 460-frame UI golden); engine split into blocks |
| 2026-11-11 | output limiter; one-bundle release scheme and per-plug-in tags |
| 2026-11-12 | ASYM saturator; SatLim built (8x detector, scope, presets) |
| 2026-11-13 | feature freeze for 0.5.2: SatXComp upward compression and lift bars, plain bypass, theme gain colours, About and changelog copy, README and screenshots |

## Regression nets
- **DSP golden** (`satcomp-dsp`): 101 scenarios rendered bit-exact against the v0.5.1 recording, audio and meter values. Re-recorded for the limiter latency and for the plain bypass in 0.5.2; the bypass scenario is the only audio change.
- **UI golden** (`satcomp-plugin/src/gui/golden.txt`): scripted frames hashed. Re-record only when a screen deliberately changes, and check that only those frames moved.
- Other tests live beside the code; `cargo test --workspace` runs all of them (pluginval strictness 5 passes for the bundles).

## Release checklist
1. Version bump in the `Cargo.toml` of each plug-in that changed (the header, About and the zip name follow it); changelog entry in the plug-in's `CHANGELOG`.
2. `cargo test --workspace`; re-record goldens only for intended changes.
3. About copy (`docs/abouttextcopy.md`) and README updated; screenshots re-shot if the UI changed.
4. `docs/releases/<tag>.md` written; this file updated (versions, history, milestones).
5. Tag on `main`, let CI build, verify the release assets and body through the API.
