# SatComp — parameter reference (internal, for audit)

**Version: v0.3** (v0.2 = grid UI, v0.1 = first prototype). Shown in the UI header (`SATCOMP 0.3`). The `desc:` line is intentionally still `SatComp` (see HANDOFF).

Describes what the code in `SatComp.jsfx` does **today**, not what the spec intends. Section/line pointers are by name (`@sample`, `gcomp`, ...) so they survive edits. Anything marked **AUDIT** is a behaviour worth a second look.

## Signal flow

```
in ──┬──────────────────────────────────────────────── dry (latency-aligned) ──┐
     │                                                                          │
     └─ [SAT HP/LP split] ─ band ─ drive+bias ─ shaper (opt. OS) ─ DC block ─ TONE ─┐
              │                                                                      ├─ SAT MIX ─ + residual ─ cL
              └─ residual (outside HP/LP band) ───────────────── delayed ───────────┘   (clean band leg ↗)
cL ─ SAT OUT (sat module master, always applied) ─ [EQ PRE] ─ [detector: FF/FB → DET HP → PEAK/RMS → LINK] ─ gain computer ─ attack/release ─ gain ─ GR DIST ─ MAKEUP
   ─ MIX (vs dry) ─ [EQ POST] ─ OUT GAIN (+ auto/match trim) ─ BYPASS (equal-loudness dry) ─ out
```

Everything between "SAT" and "MAKEUP" is skipped for sat when `Drive = 0 && Bias = 0` (see Drive).

---

## SAT

| Param | Range / default | What it does |
|---|---|---|
| **Shape** | 14 shapers, Tube | Selects the waveshaper (LUT over ±8, rebuilt on change). **Classic:** Tube = asymmetric tanh (even+odd), Soft = tanh, Rational/Atan = softer knees, Hard = clip with rounded knee at 0.9, Crossover = dead-zone, Cheby = T1+T2+T3. **Folds:** Tri Fold, Sine Fold, Asym Fold (sine fold with an offset, so it folds harder on one side → even+odd harmonics). **Aggressive:** Wrap (modulo wraparound into ±1, hard discontinuities, very aliasy — use OS x4), Rectify (positive half unchanged, negative half flipped at ½ gain → octave-up/even harmonics, DC blocked), Bitcrush (8 steps per unit ≈ 4-bit staircase, clipped ±1), Diode (exponential knee, +1/−0.6 asymmetric). All have unity small-signal slope except Crossover (unity large-signal), Rectify (mixed: 1 on the positive half, ½ flipped on the negative) and Bitcrush (stepped). **Periodic shapers** (Tri/Sine/Asym Fold, Wrap) are range-reduced instead of clamped, so they keep folding at any Drive. UI: arrows step (wrap), click name = dropdown, wheel steps. |
| **Drive** | 0–100 %, 25 | Pre-shaper gain, `10^(0.018·drive)` → 0…+36 dB (25 % = +4.5 dB). It is *gain into the shaper*, not a wet/dry amount. **AUDIT:** `sat_on = Drive > 0 \|\| Bias ≠ 0`; at Drive 0 and Bias 0 the whole sat stage is bypassed (not a unity-gain shaper), so the Drive 0 null test passes by construction. |
| **Bias** | −100…+100 %, 0 | DC offset added to the signal *after* drive, before the shaper (`x·pre + bias/100`). Skews the curve → even harmonics. A 5 Hz one-pole DC blocker follows the shaper so no DC leaks out. |
| **HP / LP** (Sat HP/LP) | 20 Hz–2 kHz / 200 Hz–20 kHz (was 1 kHz until v0.5), OFF at extremes | Band-limit what feeds the shaper (2nd-order Butterworth, 12 dB/oct). The part *outside* the band (`in − band`) skips the shaper and is added back clean (delay-matched), so HP 200 Hz = distort only above 200 Hz, lows stay clean. Not a filter on the output. |
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
| **Link** *(retired in v0.3)* | locked at 0 % | Stereo link is **no longer exposed and is fixed at 0 %** (L/R fully independent) — the expected material is mostly summed-mono drums, so it rarely matters. Slider 17 is kept (hidden, reserved) so automation/projects keep their mapping; its value is ignored. The freed grid cell now holds **Lookahead** (below). |
| **FF>FB** | 0–100 %, 0 | Detector topology. 0 = feed-forward (detect on the comp input). 100 = feedback (detect on the compressor's own previous-sample output, pre-makeup). In between = linear blend of the two detector signals. Feedback gets gentler/more "vintage" and effectively raises the ratio less than the dial says. |
| **Lookahead** *(v0.3, slider 59, cell `L.AHEAD`)* | 0–10 ms, OFF | The **detector sees the signal now, the gain is applied to a copy delayed by this time**, so the comp can already be down when the transient arrives (attack effectively starts early). Reported as plug-in latency (`pdc_delay = OS latency + lookahead`); the Mix dry leg and Bypass dry are delayed by the same total, so Mix stays phase-aligned. Delta monitors the delayed comp input minus the output. **AUDIT:** changing it while playing changes PDC and clicks (the delay line jumps); with FF>FB > 0 the feedback tap compares the delayed output with the undelayed input, so lookahead and feedback detection do not combine meaningfully. Max 1900 samples (10 ms up to 190 kHz). |
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
| **GAIN MATCH** *(v0.3, replaces MAN / AUTO / MATCH)* | off | Button above IN>OUT. Click = measures IN>OUT over 3 s of programme (button blinks, "MEAS"), then stores a static trim (±12 dB, hidden param *Match Trim*, saved with the project, shown as "LOCK", button red). Click while measuring = cancel; click while locked = off (trim ignored, value kept). **Auto was retired**: a project saved in Auto loads as Manual. Measurement point is after EQ POST, so the EQ's level change is matched too. |
| **DELTA** | toggle (red when on) | Compressor delta monitor: the output is replaced by what the compressor is removing — comp input minus comp output, taken **before** Makeup (so it includes GR Dist's effect). Silence = no gain reduction. Overrides Bypass, ignores Mix and Out Gain. UI-only (not a saved/automatable parameter) so a project can never reload silent-by-accident; it resets to off when the plug-in is reloaded. 10 ms crossfade. Output meters/PK/RMS read the delta signal while it is on. |
| **IN>OUT** | readout | Net loudness change of the whole chain: RMS (0.5 s) of the processed output vs the input, in dB, measured **after** Sat, Comp, Makeup, Mix, Out Gain and Auto/Match trim (and before Bypass/Delta). Holds its last value when the input is below −60 dBFS RMS. Centre-tap bar: grows right = output louder, left = quieter, ±12 dB full scale, tick = 0. **Note:** Auto/Match deliberately use a *separate* pre-Out-Gain measurement (`diff_db` in code); if they used this display value they would chase their own correction. So in Auto the display settles at ≈ your Out Gain setting, in Match at ≈ Out Gain once locked. |
| **TRIM** | readout | Match state (`MEAS` / `LOCK`), or `---` when off. |
| **BYPASS** | toggle | Equal-loudness bypass: crossfades (10 ms) from the processed signal to the latency-aligned dry input **scaled to the processed output's RMS** (post Out Gain). The scale (`BYP ±x dB`, clamped ±24 dB, 200 ms smoothing) is held while the input is silent. The effect keeps running while bypassed, so meters/GR stay live and unbypassing is click-free. **AUDIT:** loudness matching is RMS over 0.5 s, so it is a steady-state match; it will not be right instant-to-instant on transient material. |

### Output metering (header + info area)

- **Header meter**: L/R peak bars, −48…0 dB, white, with a 1 s peak-hold tick per channel. Measured **after** bypass and Out Gain, i.e. what leaves the plug-in. Red rect at the right end lights for 1.5 s whenever a sample exceeds 0.9999 (≈ 0 dBFS). Click the meter to reset the clip light and the held peak.
- **PK**: max sample peak since last reset (red when ≥ 0 dBFS). **RMS**: 0.3 s RMS, both channels averaged, same measurement point.

---

## UI / interaction

- Drag a cell = value (fine with Shift), mouse wheel = step (Shift = fine), double-click = reset to default. Touch/release is reported to the host for automation latch/touch.
- Layout is drawn on a fixed 560×336 logical canvas at a scale that steps in 0.25× increments (min 0.5×) and is centred. A resized window keeps proportion (letterboxed, scale follows the window in 0.25× steps; rect edges snap to whole device pixels so borders stay crisp). JSFX has no API to constrain the host window, so it cannot be hard-locked; it can only never distort.
- **Colour schemes (v0.3):** 10 palettes from `docs/colorscheme.md` (Black, White, Teal, Cadmium, Ultramarine, Azure, Magenta, Sulfur, Uranium, Midnight; each = BG / UI / ACCENT). **Left-click the header logo** = next scheme, **right-click** = dropdown with the current one ticked. Stored with the project as the scheme index (old files stored 0/1 = Black/White, same indices). Sulfur's accent equals its UI colour, so red-style highlights/fills do not stand out there by design of the palette.
- **EQ bypass dimming (v0.3):** a bypassed stage dims the 4x3 EQ grid while that stage is viewed; module bypass dims the whole EQ page (viewport + grid). Combines with the global Bypass dim.
- **Bypass dimming (v0.3):** while Bypass is on, the shape selector, OS buttons, EQ button, sat plot, comp/EQ viewport and the SAT / COMP / EQ cells draw at 50 % (colours blended toward the background); header, meters, DELTA/BYPASS and the OUT column stay at full strength. Display only.
- **Readouts (v0.3):** the comp viewport metrics (`LVL +00.0  OUT +00.0  GR 00.0dB`) and the OUT info lines (GR, TRIM/LOCK, PK, RMS, BYP: label left, value right-aligned, zero padded, placeholders `-----`) are **snapshotted ~8x/s** (`rd_*`, 0.12 s) so they are readable; meters, bars and the sat-plot SAT figure use the same snapshot cadence for numbers only. IN>OUT, MAKEUP, MIX and OUT GAIN cells are not zero padded. OUT column order: MAKEUP, MIX, OUT GAIN (bottom, inverted).
- **SAT block layout (v0.3):** DRIVE/BIAS, then `HP|LP` stacked in one cell + TONE, then SAT MIX + **OUT GAIN** (= Sat Out, slider 27, same parameter as dragging the plot). The three gain-stage cells (sat OUT GAIN, EQ OUT GAIN, output OUT GAIN) are drawn **inverted** (lit block) so they read as master levels. (black on white / white on black); red accents are unchanged. The choice is stored with the project/preset (`@serialize`), not as an automatable parameter.
- Hidden params (automatable, no on-screen widget): *Match Trim* (slider 24), *Link* (17, reserved) and all EQ sliders (29–58).
- **Header (v0.3):** logo (palette), `SATCOMP 0.3`, full-width output meter; tab row: shape selector, `[OFF|x2|x4]` (the OVERSAMPLING label was retired in v0.5), `SIDECHAIN` (v0.5), `EQ` page toggle, `DELTA | BYPASS`. The GR meter opens on its 3 dB range. Preset bar / save icon are deferred to the VST port.
- **Comp viewport (v0.3):** drag the **dot** at the threshold knee (x and y both map to Thresh), drag the **square** at the right edge (y → Ratio; hard-knee formula, so with a wide knee the handle can sit slightly off the cursor). Double-click a handle = default. The label shows live `LVL` (detector level), `OUT` (static-curve output at that level) and `GR`.

## SIDECHAIN (v0.5, Rust only)
External key for the compressor detector. A second stereo input bus ("Sidechain", nih-plug aux input) is offered by the
plug-in; hosts without sidechain support get the plain stereo layout.

| Param | Range, default | Notes |
|---|---|---|
| **Sidechain** *(63, `SIDECHAIN` button)* | off/on, off | On = the detector keys off the aux input instead of the main signal. **Not stored in presets** (routing is per track, like Bypass); is saved with the project and automatable. If on but the host provides no aux bus the detector silently falls back to the main signal and the button turns **red**. |
| **SC Gain** *(64)* | −24…+24 dB, 0 | Key level before the filters (ahead of the threshold decision). |
| **SC HP** *(65)* | 20…2000 Hz, OFF | 2nd-order Butterworth high-pass on the key. |
| **SC LP** *(66)* | 200…20k Hz, OFF | 2nd-order Butterworth low-pass on the key. |
| **Key Mix** *(67, cell `KEY MIX`)* | 0–100 %, 100 | Stands in for FF>FB while Sidechain is on (same cell). Detector level = (1−m)·main + m·key, blended as **levels** (each source has its own low-cut and peak/RMS state), so 0 % = the normal internal detector, 100 % = key only, in between ducks from both. FF>FB is not applied while Sidechain is on, whatever its value. Stored in presets. |
| **Limiter** *(68, `LIM` button in the output meter row)* | off/on, **on** | True-peak brickwall at a fixed **-0.2 dBTP**, the last stage of the processed path (after Out Gain; Bypass, Delta and Sidechain solo are not limited). Detection is 4x polyphase (2x at 96 kHz-class rates, plain sample peak above 100 kHz) with a 1.5 ms lookahead; release is program dependent (about 30 ms for isolated peaks, up to 300 ms under sustained reduction). Its gain reduction is the `LIM` readout in the output column (the compressor GR number is at the top of the GR meter). **Stored in presets**; a preset without it loads as on. The limiter's delay (1.5 ms + the detector's, about 2.2 ms at 48 kHz) is **always reported to the host, on or off**, so toggling never changes PDC. Gain Match measures before the limiter. |
| **SOLO** *(UI only, not a parameter)* | off | Replaces the output with the key as the detector hears it (after gain/HP/LP, before Low Cut and the peak/RMS blend). Clears itself when Sidechain is switched off. |

Key path: aux in > SC Gain > SC HP > SC LP > (solo tap) > detector (Low Cut, peak/RMS blend, gain computer). The key is
delayed by the oversampling latency so it lines up with the post-sat main signal; with Lookahead the detector sees it
early like it does the main signal. **FF>FB is replaced by KEY MIX while the sidechain is on** (an external key has nothing to feed back). The gain is still applied to the main signal and Mix/Delta/Bypass work as before (Delta = comp input minus output).
**UI:** with Sidechain on and the EQ page closed, the comp plot is shortened and a row `SC | HP | LP | SOLO` sits under it.
The plot x axis is then the **key** level; triangles on the left/right edges show the main level into the compressor and
after the gain reduction; the readout reads `SC / IN / GR`. The About page flow diagram adds `SIDECHAIN > SC GAIN/LP/HP`
into COMP. The tab row no longer carries the OVERSAMPLING label (OFF / x2 / x4 stay).

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
| 28 | *(unused)* | 29–43 | EQ Pre (see EQ) |
| | | 44–58 | EQ Post (see EQ) |
| 59 | Lookahead | 60–62 | EQ Pre / Post / Module bypass |
| 63 | Sidechain *(v0.5)* | 64–67 | SC Gain / SC HP / SC LP / Key Mix *(v0.5)* |
| | | 68 | Limiter *(0.5.2, default on)* |

New sliders were appended (25–27, 29–58) so existing sessions/automation keep their mapping. Old sessions should sound identical (Sat Mix 100 %, Bypass off, Sat Out 0 dB, Auto off); if REAPER restores missing sliders as 0, re-set Sat Mix to 100 % once.

---

## EQ (v0.3)

Two opt-in stages, each **3 bands + tilt + out gain**, edited on the EQ page (`EQ` button next to the OS buttons; the square inside the button is filled when either stage is active, outline when untouched).

- **PRE-COMP** sits after Sat Out, right before the comp detector (so it shapes what the compressor sees *and* what it passes on; the DELTA monitor still shows only what the comp removed). A pre-*saturator* EQ is deliberately not built-in (use an external plug-in).
- **POST-COMP** sits after Mix and before Out Gain, so IN>OUT / Gain Match include the EQ.
- **Bypass squares (v0.3):** three indicators, each clickable: the square in the `EQ` button (whole module), and one on each PRE-COMP / POST-COMP tab. Filled = has gain active, outline = untouched, **outline with an X = bypassed** (sliders 62 / 60 / 61, saved + automatable). A bypassed section is faded out over 10 ms and then not processed at all; re-enabling restarts its filters from clean state. Pre/Post bypass and Module bypass combine (either bypasses).
- A stage is **processed only when active**: any shelf/bell with gain ≠ 0, tilt ≠ 0, EQ out gain ≠ 0, or a band set to HPF/LPF. Otherwise it is skipped entirely (bit-exact null, zero CPU). The square on each PRE/POST tab uses the same rule. **Note (deviation from the spec'd rule):** switching a band to HPF/LPF counts as active even though it has no gain control, since it audibly filters.
- Filters are RBJ biquads (min-phase, no PDC). Coefficients ramp linearly over one block, so dragging does not zipper; a filter that returns to neutral ramps to identity and is then retired.

| Band | Types (icon order) | Default |
|---|---|---|
| 1 | HPF · low shelf · bell | low shelf 80 Hz |
| 2 | HPF · LPF · bell | bell 200 Hz |
| 3 | LPF · high shelf · bell | high shelf 6 kHz |

Per band: **Type**, **Freq** 20–20k (log), **Gain** ±18 dB (dashes for HPF/LPF), **Q** 0.1–10 (default 0.5; HPF/LPF = resonance, 12 dB/oct; shelf/bell = RBJ `alpha = sin/(2Q)`). **Tilt** ±12 dB = high shelf +T/2 and low shelf −T/2 around **Pivot** (20–20k, default 600 Hz), both with Q 0.4 (wide, so it reads as a tilt): + = brighter. **Out Gain** ±12 dB per stage (smoothed, 20 ms).

Slider map: Pre = 29 + (band·4 + {0 type, 1 freq, 2 gain, 3 Q}), 41 tilt, 42 pivot, 43 out gain; Post = the same +15 (44–58). **Type is stored relative to the band default (0 = default type)** so sliders that load as 0 in old projects are neutral; freq / Q / pivot that load as 0 are reset to defaults once on the first block.

Viewport: red = spectrum of the signal entering the viewed stage (mono sum, 4096-pt Hann FFT ~30×/s, dB smoothing fast-up/slow-down, +4.5 dB/oct slope compensation, fixed −100…−5 dB window), white curve = summed response of the viewed stage (without out gain), white squares = band nodes (at gain for shelf/bell, on the 0 dB line for HPF/LPF), dotted vertical line = tilt pivot. Interaction: drag a node (x = freq, y = gain), wheel on a node = Q, drag the pivot line = pivot, **drag empty space horizontally = tilt** (right = brighter, left = raises the lows; Shift = fine), double-click a node / the pivot / empty space resets it. The spectrum is only captured while the EQ page is open.
