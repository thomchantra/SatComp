# SatComp — JSFX spec v0.2 (throwaway)

Name is settled: **SatComp**. A satellite-uplink look and feel is welcome as a theme (telemetry-style labels, signal/lock indicators), within the visual rules in §6.

Personal-use REAPER JSFX: pre-drive saturator → characterful compressor, with a gain-diff readout and auto-compensation. Main use: drum chain staple, from subtle tube boost to gritty/punchy.

Reference only (do NOT copy code): BurnPressor (Flat Madness) for the idea, Michael Gruhn's "Waveshaping Distortion" for the rational shaper `(1+k)x/(1+k|x|)`. Write DSP and UI from scratch, so a later clean-room claim is trivially true.

File: `SatComp.jsfx` (single file, no @import). Stereo in/out only.

## 1. Signal flow

```
in ──┬──────────────────────────────── dry (delayed by OS latency) ──┐
     └─ [SAT: band-split → LUT shaper (OS x1/x2/x4) → tone LP] → [COMP] → makeup ─┴─ Mix ─ out gain ─ out
                                                                                    ▲
in RMS ─────────────┐                                    out RMS (pre out-gain) ────┤
                    └──────────── gain diff = out − in ─────────────────────────────┘
```

- Saturation is PRE only. There is no post-drive stage.
- Mix is a single knob, always dry/wet-complementary, never two sliders.
- NO hard clip at the output.
- Gain diff is measured on the signal before out gain (so auto-trim cannot feed back into its own measurement).

## 2. Parameters

All params are real `sliderN` (automation/Param list works). Hide them from the default JSFX UI by prefixing names with `-` and draw everything in `@gfx`.

**SAT**
| param | range | notes |
|---|---|---|
| Shape | list | see §3 |
| Drive | 0–100% | exponential taper into pre-gain, ~0–36 dB |
| Bias | −100–100% | asymmetry → even harmonics; DC blocker after shaper (always on) |
| Sat HP | 20–2000 Hz (Off at min) | band-limits what gets saturated |
| Sat LP | 1k–20k Hz (Off at max) | same |
| Tone | 1k–20k Hz | post-shaper LP, fizz control |
| OS | Off / x2 / x4 | oversampling around shaper, **Off by default** (aliasing is a legitimate crunch texture; x2 suits smooth shapes, x4 suits folds/clip) |

**COMP**
| param | range | notes |
|---|---|---|
| Thresh | −40–0 dB | |
| Ratio | 1–20, ∞ at max | |
| Attack | 0.05–100 ms | |
| Release | 5 ms–2 s, **Auto at max** | Auto = program-dependent, see §4 |
| Speed | 0–100% (α↔β) | scales all time constants (~0.25x–4x); low = fast reaction |
| Knee | 0–24 dB | |
| Knee Shape | 0–100% | knee curve family, see §4 |
| Detector | 0–100% | peak↔RMS morph |
| Det HP | Off–400 Hz | sidechain HP |
| Link | 0–100% | stereo link, 0 = independent, 100 = full (max/mean blend) |
| FF/FB | 0–100% | feedforward↔feedback detection |
| GR Dist | 0–100% | GR-coupled harmonic distortion in the gain element |

**OUT**
| param | range | notes |
|---|---|---|
| Makeup | 0–24 dB | manual |
| Mix | 0–100% | latency-compensated dry |
| Out Gain | −24–+12 dB | manual trim |
| GainMatch | Manual / Auto / Match | see §5 |

## 3. Saturator

- Shaper is a **lookup table** (e.g. 4096 pts, linear or cubic interpolation) over input range ±8 (folds need headroom). Input = `x * drive_gain + bias`, clamped to the table range. Rebuild the table only when Shape changes (in `@slider`, never in `@gfx`).
- Presets (all normalized to roughly unity small-signal gain so Drive is the only level control):
  1. Tube — asymmetric soft-sat (positive and negative halves with different knees), even-harmonic leaning
  2. Soft — tanh
  3. Rational — `(1+k)x/(1+k|x|)`, the "subtle" one
  4. Atan
  5. Hard — clip with a tiny round-off
  6. Tri Fold — triangle foldback
  7. Sine Fold — `sin(x·π/2·g)`-style fold
  8. Crossover — dead-zone/zero-crossing distortion
  9. Cheby — mix of T2 + T3 (harmonic generator)
- Band-limited saturation, phase-coherent, no crossover: `out = x − bp(x) + shaper(bp(x))`, where `bp` is HP→LP of the saturator band. Drive 0 must null perfectly.
- Gain compensation inside the shaper section is NOT done here. The gain-diff feature is the compensation tool.
- Oversampling: Off / x2 / x4, default Off. x2 = one polyphase halfband stage; x4 = two cascaded halfband stages. No x8 or higher (diminishing returns, too much CPU in JSFX). Report latency with `pdc_delay` / `pdc_bot_ch` / `pdc_top_ch` for the active setting, and delay the dry path by the same amount; changing OS mode must update both together. Latency is 0 when OS is Off.
- Read-only transfer-curve plot in the UI (draw the table with drive/bias applied). No editable curve in v1.

## 4. Compressor

Design in the dB domain. All coefficients are srate-independent and recomputed in `@slider`/`@block`.

- **Detector:** peak↔RMS morph. Detection is on the stereo-linked sidechain (HP filtered). The Link param blends per-channel and linked detection.
- **Gain computer:** threshold, ratio, knee in dB. **Knee Shape** morphs the knee curve family. The curve must stay continuous in value AND slope at both knee edges (C1). Prototype this in numpy first and plot it. Do not use a naive `t^p` as the shape; it breaks slope continuity.
- **Smoothing:** applied to GR in the dB domain with attack/release. All times scale with Speed.
- **Program-dependent release (Auto):** two release stages (fast + slow), blended by signal behavior (e.g. GR depth and/or crest factor), so sustained compression releases slower and transients release fast. Clamp the Auto range so it stays usable on drums.
- **FF/FB:** feedback mode detects from the compressor output (previous-sample, pre-makeup). Blend continuously with feedforward.
- **GR-coupled distortion (GR Dist):** inside the gain element, blend in a small saturating curve on the post-gain signal with amount proportional to the current GR (normalize, e.g. GR/12 dB, clamped). Zero GR = zero added distortion.
- **Not in v1** (note only): level-dependent attack, ratio-vs-GR (vari-mu), lookahead, detector tilt EQ.

## 5. Gain diff + auto compensation

- Running RMS of plug input and of pre-out-gain output, window ~400 ms–1 s. Diff in dB, displayed next to Out Gain, with sign.
- **Manual:** display only.
- **Auto:** the Out Gain trim follows `−diff`, heavily smoothed (about 1–2 s), clamped ±12 dB. Gate the update when the input is below about −60 dB so silence doesn't wind it up.
- **Match:** the user presses it, the plug averages the diff over ~3 s of playback, then **freezes** the trim at that value (button shows locked state; pressing again re-measures).
- Out Gain slider is the manual trim ON TOP of Auto/Match.

## 6. UI (custom @gfx)

Goals: small footprint (laptop screens), high contrast, tight spacing. Fixed layout at integer scale only (1x, optional 2x via window size), no continuous scaling.

Visual rules:
- Background solid black. All text and line/stroke elements solid white. Solid red is the only accent color (GR meter, active/locked states).
- No rounded rects anywhere: square corners only, 1px strokes.
- Font: **Fira Code** via `gfx_setfont`. Distribution/fallback is not handled now; if it isn't installed, fall back to a generic monospace (Menlo/Consolas/Courier New). Keep the font name in one constant so it's trivial to swap.
- Satellite-uplink flavor is optional flavor only (e.g. "LOCK" / "SIG" style labels), not extra UI.

Target window ~ 560×240 at 1x. Three columns:
- **SAT:** Shape selector, transfer plot, Drive/Bias, HP/LP/Tone, OS toggle
- **COMP:** Thresh, Ratio, Attack, Release, Knee, Knee Shape, Detector, Det HP, Link, Speed, FF/FB, GR Dist (compact cells, advanced params can sit in a smaller secondary row)
- **OUT:** standard GR meter (vertical bar, peak hold), gain-diff readout, Makeup, Mix, Out Gain, GainMatch mode (Manual/Auto/Match)

Polish is the user's job later; build structure, not beauty.

### Widget kit (write once, reuse)
- Single active-drag id, one hit-test helper, `mouse_cap` edge detection for press/release.
- Vertical drag = change value; **Shift = fine**; **double-click = reset to default**; mouse wheel support.
- Right-click or Ctrl-click: optional, skip for now.
- Widgets: value cell (label + readout + drag), selector (click to cycle or popup via `gfx_showmenu`), toggle, GR meter, diff readout, transfer plot.
- Writing back: set sliderN, call `sliderchange(...)`, and call `slider_automate(...)` on drag so changes record automation (check the current REAPER JSFX docs for exact signature/touch behavior).

## 7. Gotchas to respect

- JSFX functions need explicit `instance()`/`local()` declarations or variables leak into globals.
- `@gfx` runs on its own timing; no heavy DSP/table work there.
- Denormals: add a tiny DC offset or `|x| < 1e-30 → 0` in filters and envelopes.
- Recompute filter coefficients on `srate` change, not just on slider change.
- Keep CPU modest: LUT + OS (up to x4) + one compressor must be cheap, because this goes on many drum tracks. Skip the oversampling path entirely when OS is Off.

## 8. Build order

1. Skeleton: sliders, passthrough, stereo, `desc`/tags.
2. Compressor core (gain computer, smoothing, link, Det HP) + numpy plot of knee family to settle Knee Shape.
3. Saturator: LUT + presets + band-split + DC block + Tone. Then x2/x4 OS + PDC + dry delay.
4. Program-dependent release, FF/FB, GR Dist.
5. Gain diff + Auto/Match.
6. UI: widget kit → layout → meters → transfer plot.

## 9. Verification

No REAPER here, so JSFX can't be run in-session. Do:
- numpy prototypes for the gain computer (continuity checks), shaper tables (normalization, symmetry), and halfband response.
- Careful manual read of JSFX syntax and function scoping.
- Leave a short test checklist for the user in REAPER: drive 0 null test, OS Off/x2/x4 latency alignment with Mix at 50%, Auto stability on silence, automation recording from the custom UI.

## Defaults decided without asking (change if wrong)
- OS: Off / x2 / x4, Off by default. Stereo only. No lookahead. No editable curve.
- Match = ~3 s average then freeze. Auto = 1–2 s smoothing, ±12 dB clamp.

## Out of scope / later
- `gmem` gain-match pair across a whole FX chain (separate plug).
- Editable curve UI, bitmap font atlas, multiband saturation.
