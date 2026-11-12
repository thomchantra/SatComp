# SatLim 0.1

The SatComp saturator into an oversampled clipper and a true-peak brickwall limiter. Crates: `rust/satlim-dsp` (`LimEngine`),
`rust/satlim-plugin` (nih-plug shell, params from `tools/gen_lparams.py`, editor). VST3 `thomchantraSatLm`, CLAP `com.thomchantra.satlim`,
presets `*.satlim` in `~/Library/Application Support/SatLim/Presets`.

## Signal chain
`IN > SAT (shared Saturator, x1/x2/x4/x8) > lift by (CEILING - THRESHOLD) > CLIP (oversampled hard clip at the ceiling) > LIMITER > OUT GAIN > OUT`

- **OUT GAIN is after the limiter, on purpose.** The ceiling is what the limiter holds; the output gain is free (a clipped -0.4 dBTP
  can still have inter-sample peaks a downstream converter sees, so the owner pulls the output back).
- **THRESHOLD** is the input level that lands on the ceiling: the signal is lifted by `CEILING - THRESHOLD` dB. Lowering it pushes harder.
- **GRAB** (param id `clip`) 0-100 %: 0 = off. Otherwise a hard clamp at the ceiling, run at the oversampled rate, driven up to +6 dB into it. With the
  clip on the limiter mostly handles inter-sample excess.
- **L.AHEAD** on: 1.5 ms lookahead + 4x interpolated true-peak detection (constant latency, reported). Off: zero limiter latency, the gain
  follows the *sample* peak instantly (attack 0), so inter-sample overs can pass. The readout reads **TRU.PK** when on and **PK** when off (accent only when it clips; GR stays in the UI colour).
- **RELEASE** 15-80 ms: the slow end of the program-dependent release (fast end stays at 15 ms or below).
- **x1..x8** (OVERSAMPL, top of the limiter column) oversample the saturator and the clipper (latency 0 / 24 / 28 / 30 samples each). The limiter's own detection is always
  true-peak (L.AHD on).
- **Latency** = saturator oversampler + clipper oversampler + limiter lookahead, always reported; it changes with x1..x8 and L.AHD.
- **Cells** top to bottom: CEILING, THRESHOLD, GRAB, RELEASE, OUT GAIN (SAT GAIN in the sat module).
- **Meters:** segmented sat out (clip block at the top, held 1 s), GR with its value on top, stereo out VU (click the bars to cycle 48 / 24 / 12 / 6 dB, click the top block to reset the clip lamp and the peak readout).
- **SAT readout** (plot corner): click = measure the saturator's loudness change for 3 s, then write the SAT GAIN that cancels it (click again to cancel).
- **SCOPE FILL** (About > Appearance): solid bars (default) or an outline stroke along the peaks, like SatXComp's spectrum fill.
- **DELTA** = what the clipper and limiter removed (pre-clip signal minus limiter output, before OUT GAIN). **BYPASS** = the input at the
  same latency.

## Parameters (slot = UI address; the sat slots are SatComp's)
| slot | id | range | default |
|---|---|---|---|
| 1 | shape | 0-14 | 0 |
| 2 | drive | 0-100 % | 0 |
| 3 | bias | -100-100 % | 0 |
| 4 / 5 | sat_hp / sat_lp | 20-2000 / 200-20000 Hz | 20 / 20000 (off) |
| 6 | tone | 1000-20000 Hz | 20000 (off) |
| 7 | os | 0-3 (x1 x2 x4 x8) | 0 |
| 25 | sat_mix | 0-100 % | 100 |
| 27 | sat_gain | -24-12 dB | 0 |
| 12 | lookahead (L.AHEAD) | on / off | on |
| 8 | ceiling | -24-0 dB | -0.3 |
| 9 | threshold | -30-0 dB | 0 |
| 10 | release | 15-80 ms | 80 |
| 11 | clip (GRAB) | 0-100 % | 0 |
| 22 | out_gain | -24-12 dB | 0 |
| 26 | bypass | on / off | off (never in presets) |

The custom curve and ASYM per shaper are stored in the project and in presets, like SatComp.

## Tests
`satlim-dsp`: 8 engine tests (exact delayed copy at x1, latency table, bypass, ceiling held at several modes, OUT GAIN after the ceiling,
clip lightens the limiter, delta, telemetry). `satlim-plugin`: params / presets / identity + 4 headless editor tests
(`gui/smoke.rs`, scripted input against a recording fake host). Limiter additions are tested in `satcomp-dsp` (defaults identical, zero-latency
mode holds the sample ceiling and passes quiet audio bit-exactly, clip off leaves a spike to the gain stage).

## Not done / known
- The editor is tested headlessly; first-pass screenshots from REAPER came back (`docs/feedbacknotes/satlimpass1.png`) and pass 1 is applied.
- No audio golden for `LimEngine`, no M/S, no sidechain, no auto-release.
