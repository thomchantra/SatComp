---
title: SatDiff
version: 0.1.0
tagline: A/B waveshaper saturator with morph and phase colouring
screenshot: ../screenshots/satdiff.png
---

# SatDiff 0.1.0

A waveshaper saturator with phase colouring effects. Dedicated to sweet saturated sounds :>

<!--hero:satdiff-->

## 1. The idea

SatDiff is SatComp without the compressor and the EQ, and with a different kind of spice. The signal is rotated in phase, optionally smeared by a colour effect, and pushed through **two complete saturators** (scene A and scene B) that you can crossfade or morph between. A high-pass with mid / side switches and a true-peak limiter finish the chain.

<!--flow:satdiff-->

Use Tube with low drive for warm saturation, Tri Fold for destruction, or a custom asymmetric curve to annihilate.

## 2. Layout

| Area | What it does |
|---|---|
| Header | logo (themes), title (About), preset bar, window scale, BYPASS, oversampling and LIMIT |
| Scene bar | the **A \| B** tab picks the scene the cells edit; shaper selector and its arrows |
| Transfer plot | the shaper curve with the other scene underlaid, a live input trace and the pencil icon for the custom curve |
| Morph module | MORPH mode switch, DEPTH or MIX, SPEED |
| Vertical meter | scope or spectrum of the output |
| Saturator cells | DRIVE, ASYM, HPF, LPF, BIAS (with the tile buttons), TONE |
| Colour effects | on / off, type, PRE-SAT / POST-SAT, the effect's own controls, PHASE |
| Output | COLOR MIX, SAT MIX, OUT HPF (M / S), IN and OUT meters, IN GAIN, OUT GAIN |

## 3. Saturator scenes

SatDiff holds two complete saturators. Each scene has its own shaper, custom curve, tile, drive, bias, HPF / LPF, tone and ASYM. SAT MIX is shared.

The **A \| B** tab is only a view switch: the cells edit the scene you are looking at. What you hear is decided by MORPH.

| Control | What it does |
|---|---|
| Shaper | 15 waveshapers: Tube, Soft, Rational, Atan, Hard, Tri Fold, Sine Fold, Crossover, Cheby, Wrap, Rectify, Bitcrush, Asym Fold, Diode and **Custom** |
| DRIVE | how hard the signal is pushed into the shaper |
| BIAS | offsets the signal along the shaper's x axis before it is shaped: more even harmonics, and with TILE on it walks through copies of the curve |
| TILE | the three buttons under BIAS: no tiling, **mirror** tile or **repeat** tile. Tiling folds or repeats the curve so BIAS and DRIVE reach copies of it. It forces 4x oversampling (shown as `4x*`) and is dimmed for the periodic shapers (Tri Fold, Sine Fold, Wrap, Asym Fold). |
| ASYM | how much of the shaper's even-order part is kept. Lowering it keeps lopsided curves from dragging DC and sub-bass into the signal. Stored per shaper. |
| HPF / LPF | decide which part of the signal is saturated; the rest passes clean |
| TONE | low-pass on the shaped signal only |
| SAT MIX | blend between clean and saturated |

**Custom curve.** Click the pencil icon in the plot corner to draw your own curve with up to 30 points. Drag the hollow handles to bend a segment; double-click one to straighten it. Pick **LINEAR** or **CURVE** per scene, and mirror the curve odd or even with the tools at the top right of the plot.

**Level match.** Click the **SAT** readout in the plot corner to set OUT GAIN so the saturator does not change the level.

## 4. Morph

| Mode | What it does |
|---|---|
| **XFADE** | a static crossfade between the two scenes. DEPTH sets the blend: 0 % is scene A only, 100 % is scene B only. |
| **MORPH A>B** / **B>A** | a compressor-style detector follows the input level and moves the blend: scene A on silence, scene B on loud peaks (or the other way around). **DEPTH** sets how far it moves; **SPEED** sets the attack and release of the detector. |

Click the morph button to cycle the mode, or right-click it for the list. A thin line at the bottom of the button shows the live position. MORPH blends the **outputs** of two full saturator chains, so each scene keeps its own drive, filters and tone. Scene B only runs while the blend is above 0.

## 5. Phase and colour effects

**PHASE** rotates the phase of the signal (-180 to +180 degrees, zero latency) before the saturator. The saturator is level dependent, so rotating the phase changes where the peaks fall and what it makes of them.

The **colour slot** adds one effect, before (**PRE-SAT**) or after (**POST-SAT**) the saturators. **COLOR MIX** sets how much of it you hear. Every effect has zero latency.

| Effect | What it does |
|---|---|
| DIFFUSOR | a stack of allpass stages (up to 100). STAGES sets how many, SPREAD fans them out over up to 8 octaves around FREQ, Q shapes them. Stacked stages only lag the bass; spreading them is what scrambles the harmonics. |
| PHASER | 2 to 12 allpass stages swept by a free-running LFO: RATE, DEPTH, FEEDBACK, WIDTH |
| FLANGER | a modulated short delay with feedback (no through-zero), RATE, DEPTH, FEEDBACK, DELAY |
| RINGMOD | an internal carrier (sine, triangle, square or saw) multiplies the signal; AMOUNT goes from AM to full ring. HPF and LPF shape the wet signal. |
| FREQ SHIFT | a frequency shifter, -1000 to +1000 Hz, with feedback; WIDTH counter-shifts the right channel. HPF and LPF shape the wet signal. |

The LFOs run freely in Hz; there is no tempo sync. Changing the type fades between effects.

## 6. Output

| Control | What it does |
|---|---|
| IN GAIN | first in the chain, -24 to +24 dB. The IN meter reads the dry input before it. |
| OUT HPF (M / S) | a 24 dB per octave high-pass right before OUT GAIN, with **M** and **S** switches for the mid and side signals. Side only is a mono sub: it cleans up the low end of the stereo image and leaves the centre alone. |
| OUT GAIN | final level, -24 to +12 dB |
| LIMIT | true-peak output limiter at -0.2 dBTP. It is the last stage, after OUT GAIN. |
| 1x / 2x / 4x / 8x | oversampling of the saturators |
| BYPASS | plain bypass at the plug-in's latency |

The **IN** and **OUT** meters carry clip lamps. The vertical meter beside the morph module is a scope; **left-click** it to cycle the timing windows and **right-click** to switch to the spectrum.

## 7. Recipes

**Warm glue.** Tube with 10-20 % drive, SAT MIX around 80 %, a gentle POST-SAT diffusor at low COLOR MIX.

**Drums that breathe.** Scene A soft and clean, scene B Tri Fold with high drive. MORPH A>B at 60 % depth and a medium SPEED: the saturator folds only on the hits.

**Wide and clean low end.** Put the colour effect after the saturator, then set OUT HPF to 120 Hz with only **S** on. The lows stay mono, the highs keep their width.

**Comb into tone.** Switch TILE to repeat with BIAS around 30 %: the offset walks through copies of the curve and adds new harmonics at the same drive.

## 8. Good to know

- **Latency** is reported to the host and follows the oversampling setting.
- **Presets:** eight factory presets ship with the plug-in; yours are saved to `SatDiff/Presets` in your application-support folder. A name in asterisks means the preset has been modified.
- **Themes:** click the logo for the next theme, double-click it for dark / light, right-click for the list. The title opens About, Preferences and the changelog.
- Double-click any slider to reset it.

## Support and links

- Source code, issues and releases: [github.com/thomchantra/SatComp](https://github.com/thomchantra/SatComp)
- Support the project: [ko-fi.com/thomchantra](https://ko-fi.com/thomchantra)

Found a bug or have an idea? Open an issue on GitHub. Thank you for using SatDiff.

## Notes to check (delete before the PDF)

- Preset directory name `SatDiff/Presets` and the preset count (eight).
- Latency wording, and whether the limiter ceiling is -0.2 dBTP as in SatComp.
- MORPH mode names on the button (XFADE / MORPH A>B / B>A) and the exact tile button wording.
- Recipe values are suggestions.
