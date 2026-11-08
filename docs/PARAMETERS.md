# SatComp — parameter reference (internal, for audit)

**Version: v0.2** (v0.1 = first prototype). Shown next to the title in the UI.

Describes what the code in `SatComp.jsfx` does **today**, not what the spec intends. Section/line pointers are by name (`@sample`, `gcomp`, ...) so they survive edits. Anything marked **AUDIT** is a behaviour worth a second look.

## Signal flow

```
in ──┬──────────────────────────────────────────────── dry (latency-aligned) ──┐
     │                                                                          │
     └─ [SAT HP/LP split] ─ band ─ drive+bias ─ shaper (opt. OS) ─ DC block ─ TONE ─┐
              │                                                                      ├─ SAT MIX ─ + residual ─ cL
              └─ residual (outside HP/LP band) ───────────────── delayed ───────────┘   (clean band leg ↗)
cL ─ SAT OUT (sat module master, always applied) ─ [detector: FF/FB → DET HP → PEAK/RMS → LINK] ─ gain computer ─ attack/release ─ gain ─ GR DIST ─ MAKEUP
   ─ MIX (vs dry) ─ OUT GAIN (+ auto/match trim) ─ BYPASS (equal-loudness dry) ─ out
```

Everything between "SAT" and "MAKEUP" is skipped for sat when `Drive = 0 && Bias = 0` (see Drive).

---

## SAT

| Param | Range / default | What it does |
|---|---|---|
| **Shape** | 14 shapers, Tube | Selects the waveshaper (LUT over ±8, rebuilt on change). **Classic:** Tube = asymmetric tanh (even+odd), Soft = tanh, Rational/Atan = softer knees, Hard = clip with rounded knee at 0.9, Crossover = dead-zone, Cheby = T1+T2+T3. **Folds:** Tri Fold, Sine Fold, Asym Fold (sine fold with an offset, so it folds harder on one side → even+odd harmonics). **Aggressive:** Wrap (modulo wraparound into ±1, hard discontinuities, very aliasy — use OS x4), Rectify (positive half unchanged, negative half flipped at ½ gain → octave-up/even harmonics, DC blocked), Bitcrush (8 steps per unit ≈ 4-bit staircase, clipped ±1), Diode (exponential knee, +1/−0.6 asymmetric). All have unity small-signal slope except Crossover (unity large-signal), Rectify (mixed: 1 on the positive half, ½ flipped on the negative) and Bitcrush (stepped). **Periodic shapers** (Tri/Sine/Asym Fold, Wrap) are range-reduced instead of clamped, so they keep folding at any Drive. UI: arrows step (wrap), click name = dropdown, wheel steps. |
| **Drive** | 0–100 %, 25 | Pre-shaper gain, `10^(0.018·drive)` → 0…+36 dB (25 % = +4.5 dB). It is *gain into the shaper*, not a wet/dry amount. **AUDIT:** `sat_on = Drive > 0 \|\| Bias ≠ 0`; at Drive 0 and Bias 0 the whole sat stage is bypassed (not a unity-gain shaper), so the Drive 0 null test passes by construction. |
| **Bias** | −100…+100 %, 0 | DC offset added to the signal *after* drive, before the shaper (`x·pre + bias/100`). Skews the curve → even harmonics. A 5 Hz one-pole DC blocker follows the shaper so no DC leaks out. |
| **HP / LP** (Sat HP/LP) | 20 Hz–2 kHz / 1–20 kHz, OFF at extremes | Band-limit what feeds the shaper (2nd-order Butterworth, 12 dB/oct). The part *outside* the band (`in − band`) skips the shaper and is added back clean (delay-matched), so HP 200 Hz = distort only above 200 Hz, lows stay clean. Not a filter on the output. |
| **Tone** | 1–20 kHz, OFF | 2nd-order LP on the **shaped band only** (after the shaper + DC block, before Sat Mix). Tames the harmonics; the clean residual is unaffected. |
| **OS** | Off / x2 / x4 | Halfband oversampling around the shaper only (comp/filters run at base rate). Reports PDC latency 0 / 24 / 28 samples; the dry path, residual and sat-mix dry leg are delayed by the same amount. |
| **Sat Mix** *(new)* | 0–100 %, 100 | Parallel blend inside the sat stage: clean band (pre-drive, delay-matched) ↔ shaped+toned band. 100 % = previous behaviour; 0 % = sat stage inaudible even with Drive up. Happens **before** the compressor, so the comp sees the blended signal. **AUDIT:** no level compensation on the wet leg — Drive still adds up to +36 dB gain to the wet, so low Sat Mix with high Drive gets louder as you raise Sat Mix. Intended to be levelled by Auto/Match gain mode downstream. |

### Sat Out / sat level-match

| Param | Range / default | What it does |
|---|---|---|
| **Sat Out** *(slider 27)* | −24…+12 dB, 0 | Master level of the whole sat module, applied **after** Sat Mix and the clean residual, so it trims the module output whether it is fully wet or dry, and it is therefore also the **comp input level**. Plain gain: lowering it makes the wet path quieter (Mix dry stays untouched; IN>OUT shows the net effect). Applied even when the sat stage is bypassed (Drive 0 + Bias 0), where it is simply a pre-comp trim. Smoothed (20 ms). **UI:** drag the SAT plot vertically (Shift = fine, wheel steps, double-click resets). A marker on the plot's left edge shows the level (dotted line while dragging); `OUT ±x dB` in the top-left shows the value. |
| **Sat level-match** *(UI action, no parameter)* | click the bottom-right `SAT ±x dB` readout | Same flow as the out-stage **Match**: starts a **3 s measurement** (readout blinks `MEAS x.xs`) of the sat module's loudness change (RMS, 0.5 s window, output vs plug-in input, before Sat Out, counted only while the input is above the −60 dB gate), then **writes `−average` into Sat Out** (clamped −24…+12, rounded to 0.1 dB) and stops. It does not stay engaged, so it cannot act as a slow compressor; re-click to re-measure, click during a measurement to cancel (Sat Out unchanged). Replaces the continuous "auto" from the first v0.2 draft. |

Live viz (SAT plot): `SAT ±x dB` (bottom-right, click = 3 s level-match) is the sat module's loudness change — RMS (0.5 s) of its output vs the plug-in input, before Sat Out; use it to set Sat Out by hand. Red portion of the curve = region currently reached by the input peak (pre-drive, post sat HP/LP, ~150 ms decay); dots mark the + and − ends. Axis is ±1 full-scale *input*, so with Drive up the dots sit on the flat part sooner.

---

## COMP

Detector chain order (per sample): **FF/FB mix → Det HP → peak/RMS blend → Link → gain computer → attack/release smoothing**.

| Param | Range / default | What it does |
|---|---|---|
| **Thresh** | −40…0 dB, −18 | Level where gain reduction starts (centre of the knee). |
| **Ratio** | 1–20, 4:1 | Slope above threshold: GR slope `= 1 − 1/ratio`. ≥ 19.9 displays **INF** and acts as a limiter (slope 1). |
| **Attack** | 0.05–100 ms, 10 | Time constant (63 %) of the one-pole smoother on the GR signal **in dB**, used when GR is increasing. Scaled by Speed. |
| **Release** | 5–2000 ms, 150 | Same, when GR is decreasing. Top of the range (≥ 1990) = **AUTO**: runs a fast (40 ms) and slow (600 ms) release in parallel and crossfades by weight 0.1…0.85. Weight goes toward slow when average GR is deep and sustained, toward fast when the signal is transient (crest factor ≥ ~5 forces fast). Scaled by Speed. |
| **Speed** | 0–100 %, 50 | Global time multiplier `4^((s−50)/50)`: 0 % = 0.25× (everything 4× faster), 50 % = 1×, 100 % = 4× slower. Applies to attack, release and both AUTO stages. |
| **Knee** | 0–24 dB, 6 | Width of the soft knee, centred on Thresh (GR begins at `thr − knee/2`, full slope at `thr + knee/2`). 0 = hard knee. |
| **K.Shape** | 0–100 %, 0 | Knee curve family, C1-continuous at both ends: 0 = quadratic (classic), 50 = smoothstep, 100 = smootherstep (more gradual onset, steeper middle). |
| **Detect** *(label: "PEAK>RMS")* | 0–100 %, 30 | Detector type blend: 0 % = instantaneous peak `\|x\|`, 100 % = RMS (10 ms window, scaled ×√2 so a sine reads the same as its peak). |
| **Det HP** *(label: "LOW CUT")* | OFF–400 Hz, OFF | 12 dB/oct HP in the **detector path only** (audio is untouched). Raising it makes the comp ignore low end so kicks/bass stop driving GR. Shown OFF ≤ 15 Hz. |
| **Link** | 0–100 %, 100 | Stereo link. Each channel's detector level is pulled toward the louder of L/R: `lv += link·(max − lv)`. 100 % = identical GR on both channels; 0 % = fully independent. **AUDIT:** ≥ 99.9 % takes a shortcut and applies L's computed gain to R. |
| **FF>FB** | 0–100 %, 0 | Detector topology. 0 = feed-forward (detect on the comp input). 100 = feedback (detect on the compressor's own previous-sample output, pre-makeup). In between = linear blend of the two detector signals. Feedback gets gentler/more "vintage" and effectively raises the ratio less than the dial says. |
| **GR Dist** | 0–100 %, 0 | Soft saturation (`x/(1+0.5\|x\|)`) applied **after** gain reduction, amount `GRDist · min(GR/12 dB, 1)` per channel — grit that grows with how hard the comp is working; zero when GR = 0. Post-comp, pre-makeup, and *inside* the feedback tap when FF>FB > 0. |

Live viz (TRANSFER plot): red fill under the curve up to the current detector level (L/R max after FF/FB, Det HP and Detect blend, **before** Link, ~150 ms decay); the dot is the operating point on the curve; the tick at the bottom is Thresh. Dotted line = 1:1.

GR meter (right of the plot): shows max(L,R) GR, with peak-hold line (1 s hold, then falls). Click cycles the scale 3 / 6 / 12 / 18 dB; it auto-widens when GR exceeds the range but never narrows on its own (click to wrap back down).

---

## OUT

| Param | Range / default | What it does |
|---|---|---|
| **Makeup** | 0–24 dB, 0 | Gain after the comp (and after GR Dist). Feedback tap is taken *before* this. |
| **Mix** | 0–100 %, 100 | Global dry/wet: linear blend of the latency-aligned **input** (before sat) and the full wet chain (sat + comp + makeup). **AUDIT:** because dry is the raw input, Mix < 100 % also bypasses the saturation, not just the comp. Not level-compensated. |
| **Out Gain** | −24…+12 dB, 0 | Final gain, plus the Auto/Match trim below. Smoothed (20 ms). |
| **MAN / AUTO / MATCH** | Manual | Loudness-matching mode for the output: **MAN** = Out Gain only. **AUTO** = continuously trims by `−IN>OUT` (clamped ±12 dB, ~1.5 s time constant), frozen while the input is silent. **MATCH** = measures IN>OUT over 3 s of programme (button blinks, "MEAS"), then stores a static trim (±12 dB, hidden param *Match Trim*, saved with the project, shown as "LOCK"). Clicking MATCH again re-measures. |
| **DELTA** | toggle (red when on) | Compressor delta monitor: the output is replaced by what the compressor is removing — comp input minus comp output, taken **before** Makeup (so it includes GR Dist's effect). Silence = no gain reduction. Overrides Bypass, ignores Mix and Out Gain. UI-only (not a saved/automatable parameter) so a project can never reload silent-by-accident; it resets to off when the plug-in is reloaded. 10 ms crossfade. Output meters/PK/RMS read the delta signal while it is on. |
| **IN>OUT** | readout | Net loudness change of the whole chain: RMS (0.5 s) of the processed output vs the input, in dB, measured **after** Sat, Comp, Makeup, Mix, Out Gain and Auto/Match trim (and before Bypass/Delta). Holds its last value when the input is below −60 dBFS RMS. Centre-tap bar: grows right = output louder, left = quieter, ±12 dB full scale, tick = 0. **Note:** Auto/Match deliberately use a *separate* pre-Out-Gain measurement (`diff_db` in code); if they used this display value they would chase their own correction. So in Auto the display settles at ≈ your Out Gain setting, in Match at ≈ Out Gain once locked. |
| **TRIM** | readout | Current Auto trim (`TRIM +x dB`), Match state (`MEAS` / `LOCK`), or `---` in Manual. |
| **BYPASS** | toggle | Equal-loudness bypass: crossfades (10 ms) from the processed signal to the latency-aligned dry input **scaled to the processed output's RMS** (post Out Gain). The scale (`BYP ±x dB`, clamped ±24 dB, 200 ms smoothing) is held while the input is silent. The effect keeps running while bypassed, so meters/GR stay live and unbypassing is click-free. **AUDIT:** loudness matching is RMS over 0.5 s, so it is a steady-state match; it will not be right instant-to-instant on transient material. |

### Output metering (header + info area)

- **Header meter**: L/R peak bars, −48…0 dB, white, with a 1 s peak-hold tick per channel. Measured **after** bypass and Out Gain, i.e. what leaves the plug-in. Red rect at the right end lights for 1.5 s whenever a sample exceeds 0.9999 (≈ 0 dBFS). Click the meter to reset the clip light and the held peak.
- **PK**: max sample peak since last reset (red when ≥ 0 dBFS). **RMS**: 0.3 s RMS, both channels averaged, same measurement point.

---

## UI / interaction

- Drag a cell = value (fine with Shift), mouse wheel = step (Shift = fine), double-click = reset to default. Touch/release is reported to the host for automation latch/touch.
- Layout is drawn on a fixed 560×336 logical canvas at a scale that steps in 0.25× increments (min 0.5×) and is centred. A resized window keeps proportion (letterboxed, scale follows the window in 0.25× steps; rect edges snap to whole device pixels so borders stay crisp). JSFX has no API to constrain the host window, so it cannot be hard-locked; it can only never distort.
- **Palette:** the header **LIGHT/DARK** button inverts the UI (black on white / white on black); red accents are unchanged. The choice is stored with the project/preset (`@serialize`), not as an automatable parameter.
- Hidden params (automatable, no on-screen widget): *Match Trim* (slider 24).

## Parameter ↔ slider map

| # | Name | # | Name |
|---|---|---|---|
| 1 | Shape | 14 | Knee Shape |
| 2 | Drive | 15 | Detector |
| 3 | Bias | 16 | Det HP |
| 4 | Sat HP | 17 | Link |
| 5 | Sat LP | 18 | FF/FB |
| 6 | Tone | 19 | GR Dist |
| 7 | OS | 20 | Makeup |
| 8 | Thresh | 21 | Mix |
| 9 | Ratio | 22 | Out Gain |
| 10 | Attack | 23 | GainMatch |
| 11 | Release | 24 | Match Trim (hidden) |
| 12 | Speed | 25 | Sat Mix *(new)* |
| 13 | Knee | 26 | Bypass *(new)* |
| | | 27 | Sat Out *(new)* |

New sliders were appended (25–27) so existing sessions/automation keep their mapping. Old sessions should sound identical (Sat Mix 100 %, Bypass off, Sat Out 0 dB, Auto off); if REAPER restores missing sliders as 0, re-set Sat Mix to 100 % once.
