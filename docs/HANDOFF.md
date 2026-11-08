# SatComp — handoff notes (for the next session / agent)

State at end of the v0.2 session. Read this first, then `docs/PARAMETERS.md` (what every control does, with AUDIT flags) and `docs/TEST_CHECKLIST.md`.

## What this is
`SatComp.jsfx` — a REAPER JSFX saturator + compressor with a fully custom black/white/red monospace UI (Fira Code, falls back to Menlo/Consolas/Courier). Free, GitHub-bound; GPL is not a concern for the author. Owner is Trirat; they test in REAPER on macOS (retina and non-retina) and send annotated screenshots + a running list of feedback. No JSFX compiler exists in this environment: **nothing can be run here**. All verification so far was (a) paren-balance/no-exponent-literal checks (`python3 -I` regex scripts) and (b) the user loading it in REAPER and reporting back. Always say it is untested until they confirm.

## Version
- **v0.1**: first prototype (sat + comp + gain diff, 560x240 UI).
- **v0.2** (current, label shown in UI header): grid UI rewrite, bypass, output metering, GR meter ranges, comp/sat live viz, Sat Mix, Sat Out, delta monitor, 14 shapers, fractional UI scale, light/dark palette.

## Signal flow (details in PARAMETERS.md)
in → [Sat HP/LP split] → drive+bias → shaper (opt. OS) → DC block → Tone → **Sat Mix** (blend vs clean band) → + clean residual → **Sat Out** (sat-module master / comp input level) → detector (FF/FB → Det HP → peak/RMS → Link) → gain computer → attack/release → GR Dist → Makeup → Mix (vs raw dry) → Out Gain (+ Auto/Match trim) → Bypass (equal-loudness) → Delta monitor → out.

## Decisions worth knowing (and why)
- **Two IN>OUT measurements.** `diff_db` (pre Out Gain) drives Auto/Match; `diff_disp` (post Out Gain/trim) is the readout. Merging them makes Auto chase its own correction.
- **Sat level-match is a one-shot 3 s measure-and-commit** (click the `SAT ±x dB` readout in the sat plot) that writes Sat Out, like out-stage Match. A continuous version was tried and rejected: left on, it acts as a slow compressor.
- **Sat Out sits at the master of the sat module**, after Sat Mix, so it affects the comp input whether sat is wet or dry; the Mix dry leg is raw input and is unaffected.
- **Delta and palette are UI-only state** (delta not saved; palette via `@serialize`), so a project can never reload silent.
- **Periodic shapers** (Tri/Sine/Asym Fold, Wrap) are range-reduced instead of clamped at the ±8 LUT edge, so they keep folding at any Drive.
- **Lockhart fold was swapped for "Asym Fold"** (a faithful one needs a Lambert-W cascade). Revisit if wanted.
- **Window proportion cannot be locked in JSFX.** UI draws on a 560x336 logical canvas, scale in 0.25 steps (min 0.5), letterboxed; rect edges snap to device px. Hard aspect-lock is a VST3/CLAP thing.
- Naming: Detect → **PEAK>RMS**, Det HP → **LOW CUT** (the user chose these over "low/hi sens").

## Gotchas learned the hard way (EEL2 / JSFX)
- **Deep nested ternary chains break.** One 26-deep `sget/sput` chain left sliders 25/26 unwritable (value stuck at 0, drag highlighted but didn't stick). Fixed by splitting into chains of ≤10 (`sget_a/b/c`, `sput_a/b/c`). **Keep any new slider-dispatch chains short.**
- No exponent literals (`1e-30`): write `0.000000000000000000000000000001`.
- Sliders added to an existing project can load as 0 (Sat Mix showed 0% in an old session). New sliders must be *appended* to keep automation mapping; tell the user to re-set defaults once.
- `%` in EEL2 truncates to int — fine for the meter label test, don't use on non-integers.
- `gfx_clear` colour is `r + g*256 + b*65536`; white = 16777215.
- Don't change `desc:` (changes the FX identity in REAPER).
- Edits that make the file "change on disk" are usually the user tweaking (they edit font sizes in `pick_font()`); re-read before editing.

## Slider map
1 Shape(0–13) 2 Drive 3 Bias 4 SatHP 5 SatLP 6 Tone 7 OS | 8 Thresh 9 Ratio 10 Attack 11 Release 12 Speed 13 Knee 14 KneeShape 15 Detector(PEAK>RMS) 16 DetHP(LOW CUT) 17 Link 18 FF/FB 19 GRDist | 20 Makeup 21 Mix 22 OutGain 23 GainMatch 24 MatchTrim(hidden) 25 SatMix 26 Bypass 27 SatOut. Slider 28 was removed (old auto toggle).

## Open ideas / next up (from the owner)
1. **UI refresh** — owner is doing this themselves next.
2. **Opt-in QoL: two 3-band EQs, pre-comp and post-comp** (not started). Design notes: keep them bypassed by default (zero-cost, bit-exact null); biquads TDF2 already exist (`bq`, `bq_lp`, `bq_hp`; need peaking/shelf coeffs); decide whether "pre-comp" sits before the sat or between Sat Out and the detector (ask); PDC unaffected (minimum-phase); watch the slider-chain depth and the UI grid (it is full — a collapsible panel or tab is probably needed).
3. **Port to VST3 (+CLAP) in Rust** — owner wants a crash course next session. Recommended stack: `nih-plug` (VST3+CLAP), egui for the GUI (custom painted rects/mono text, embed Fira Code, OFL), `clap-wrapper` for AU if wanted. VST2 is not an option. Plan: freeze features → port DSP as a standalone crate → null-test against JSFX renders (the numpy scripts in `proto/` are reference implementations) → plugin shell (params, PDC/latency 24/28 samples, atomics for meters, 3 s measurement flows) → UI → packaging/signing. Aspect-locked resize is possible in VST3/CLAP, which resolves the JSFX limitation. Check current licensing of the VST3 bindings, though the owner says GPL is fine. Next session should start with an intro-level walk-through (crates, nih-plug project layout, params/smoothing, process loop, egui editor, validator tools: pluginval, clap-validator).
4. Owner wants **peer feedback**: when ready, a short README for GitHub (install path `Effects/`, screenshot, v0.2 feature list, the PARAMETERS.md link) would help. Not written yet.

## Unverified / audit candidates
See the **AUDIT** flags in `PARAMETERS.md` (Drive 0 + Bias 0 skips the sat stage entirely; Mix < 100 % also bypasses sat; Link ≥ 99.9 % reuses L's gain for R; Sat Mix wet leg has no level compensation by design; bypass loudness match is steady-state RMS). Test items 13–27 in `TEST_CHECKLIST.md` cover everything added this session. The owner has reported back positively on the grid layout, Sat Mix/Bypass (after the slider-chain fix), the meters, Delta, and the Sat Out + auto-level idea (Sat Out/IN>OUT post-gain were in their hands when they asked for the one-shot change). **Not yet reported on:** fractional UI scaling on non-retina, the 5 new shapers, the one-shot sat level-match, the LIGHT palette, `@serialize` persistence.
