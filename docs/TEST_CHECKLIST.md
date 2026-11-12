# SatComp — first-pass test checklist (run in REAPER)

Install: copy/symlink `SatComp.jsfx` into `Effects/`, insert as `JS: SatComp`. **First thing: does it compile?** (If the FX window shows an error line, send it over.)

1. **Compile + UI**: window opens ~560x240, black/white/red, text in Fira Code (else Menlo). Resize window to 1120x480 -> UI snaps to 2x.
2. **Drive 0 null**: Drive 0, Bias 0, Mix 100, GR Dist 0, Ratio 1 (or Thresh 0). Duplicate item with polarity flipped -> silence. Repeat with OS x2/x4 (null should hold, delayed identically).
3. **Latency**: Mix 50, OS Off -> x2 -> x4 on a kick; FX chain PDC readout should show 0 / 24 / 28 samples; no comb filtering / flam when switching.
4. **Shapes**: step through all 9 on a sine at -12 dBFS, Drive ~60: check spectrum; Tube has even harmonics, folds fold, Cheby adds H2/H3. Transfer plot matches.
5. **Sat HP/LP/Tone**: HP 200 Hz keeps bass clean; LP/Tone remove fizz. OFF shown at extremes.
6. **Compressor**: Thresh -20, Ratio 4, Knee 12: drag Knee Shape — curve in the bottom plot changes smoothly, no zipper. Ratio at max -> INF limits. GR meter moves, peak-hold line falls after ~1 s.
7. **Release AUTO** (drag Release to top): kick loop = fast recovery, sustained pad = slow. Speed low = faster everything.
8. **FF/FB**, **Link** (hard-panned hit: Link 0 leaves other side alone), **Det HP**, **Detector** morph: audible/plausible.
9. **GR Dist**: 0 = no change; raise with GR > 0 -> added grit; GR = 0 -> clean.
10. **Gain diff**: IN>OUT readout stable on steady program; **Auto**: trim settles in ~2-4 s, stays put on silence (gate), no wind-up. **Match**: press while playing -> blinking ~3 s, then red LOCKED value; press again re-measures; value survives save/reload.
11. **UI**: drag = value, Shift = fine, double-click = default, wheel works, Shape menu opens. **Automation**: touch/latch record from the custom UI writes envelopes; playback moves the UI.
12. **CPU**: 10+ instances on a drum bus with OS x4 — note CPU.

## UI pass 2 additions
13. **Layout**: window opens 1120x672 (560x336 @2x). Resize freely: layout stays proportional, letterboxed, never stretches. Check every label fits its cell.
14. **Shape selector**: arrows step and wrap Cheby<->Tube, click name opens dropdown, wheel steps.
15. **Comp viz**: red fill follows input level on the transfer plot; dot rides the curve; fill empties on silence.
16. **Sat viz**: red part of the curve widens with level; dots reach the flat part with high Drive.
17. **Sat Mix**: Drive 60, Sat Mix 100 -> 0: sat fades out, no click, no level jump at 0 other than the drive gain. OS x2/x4: no comb when sweeping (dry leg is delay-matched).
18. **Bypass**: toggles click-free; loudness stays within ~1 dB on steady material; BYP readout shows the applied gain; GR meter keeps moving while bypassed.
19. **Out meter**: bars follow output, hold tick falls after 1 s, push Out Gain +12 on loud material -> red rect lights; click meter clears it. PK/RMS readouts sane.
20. **GR meter**: click cycles 3/6/12/18; heavy GR auto-widens the range; ticks/labels line up (labels at 1/2/3, 2/4/6, 3/6/9/12, 6/12/18).
21. **IN>OUT bar**: bar grows right for boost, left for cut, centre tick fixed.
22. **New shapers** (Wrap, Rectify, Bitcrush, Asym Fold, Diode): step through on drums with Sat Mix ~30-50 %. Check each at Drive 100 and OS x4 vs Off (Wrap/Bitcrush alias heavily at OS Off). Tri/Sine/Asym Fold + Wrap must keep folding (not freeze to a constant) at Drive 100 on a hot signal. Dropdown lists all 14, wrap-around arrows go Diode<->Tube.
23. **DELTA**: click DELTA (turns red) with GR > 0 -> you hear only the removed signal (e.g. kick transients duck-shaped); GR = 0 / ratio 1 -> silence. Toggle is click-free. Makeup/Mix/Out Gain don't change what you hear; raising Drive changes it only via the comp input.
24. **IN>OUT after Out Gain**: Drive up so output is loud; drag Out Gain down -> IN>OUT follows 1:1. In Auto the display ~ your Out Gain value; Auto/Match trims still settle as before (no hunting/oscillation).
25. **SAT OUT**: drag the sat plot up/down: marker moves, `OUT` readout follows, plot outline red while dragging, double-click resets, wheel steps. With Drive high, lowering Sat Out reduces GR (comp sees less level); with Sat Mix 0 % it still changes the comp input level. Mix dry leg unaffected. Works with Drive 0/Bias 0 too.
26. **Sat level-match (one-shot)**: Drive up, play loud programme, click `SAT +x dB` readout -> blinks `MEAS`, after 3 s `OUT` jumps to about -x and `SAT` readout (pre-trim) stays put; click again mid-measure cancels with no change; Sat Out stays where it landed (no slow drift); manual drag afterwards works.
27. **Palette + version**: `v0.2` next to SATCOMP; LIGHT button -> black-on-white (text on lit buttons/tabs legible, red clip/GR/viz still visible), button reads DARK; save + reload project -> palette persists.

## v0.3 additions (all untested)
28. **Load**: file compiles, window opens, header shows logo + `SATCOMP 0.3`; old project loads with EQ neutral (no filtering, EQ button square outline). Clicking the logo toggles palette; palette persists.
29a. **Colour / light mode**: double-click the icon toggles light/dark of the same scheme; single click (after a short pause) cycles schemes; right-click menu has `MODE: DARK/LIGHT` on top; reopening the window keeps both.
29c. **About / Changelog**: wheel scrolls (3 rows per notch), thumb on the right drags, tab switch resets to top; light mode inverts the icon (UI-coloured plate, BG-coloured glyph).
29b. **Sidechain** (0.5): SIDECHAIN toggle, SC row under the comp plot, KEY MIX in place of FF>FB, SOLO.
29. **Tab row**: OFF/x2/x4 buttons (no label since v0.5) set OS; DELTA | BYPASS behave as before; GR meter opens at 3 dB range; header meter spans the column.
30. **GAIN MATCH**: click -> blinks ~3 s -> red LOCK; click again -> off; click mid-measure cancels. IN>OUT readout sane; Auto gone.
31. **Link**: stereo-unlinked behaviour (hard-panned hit does not duck the other side); cell at comp bottom-left is empty.
32. **Comp viewport**: drag dot = Thresh (cell follows), drag square = Ratio (to the top = INF); double-click resets; label shows LVL/OUT/GR and moves with the signal.
33. **EQ null**: EQ page closed/untouched -> output bit-identical to v0.2 (null test). Touch only a band's Freq/Type -> still neutral and square stays outline.
34. **EQ bands**: raise band gains -> square fills, response curve + audible change; type icons switch (HPF/LPF at 0 dB line, gain shows `---`); Q by drag/wheel; no zipper noise when dragging nodes during playback.
35. **Tilt / pivot**: drag empty viewport right = brighter, left = more lows; `TILT` label updates; drag dotted pivot line moves the tilt centre; double-click resets.
36. **Stages**: PRE tab changes what the comp reacts to (boost lows -> more GR); POST tab changes the output and IN>OUT/Gain Match compensate it; each tab square shows its own activity; Out Gain per stage works.
37. **Spectrum**: red fill follows the signal, PRE vs POST differ by the compression; check low-end resolution, level window (-100..-30 dB) and slope; idle CPU unchanged with the EQ page closed.
38. **Automation**: touch/latch on an EQ node drag records freq+gain envelopes; check sliders 57/58 (high slider masks) record and play back.
39. **Colour schemes**: left-click logo cycles all 10; right-click shows the dropdown (current ticked) and picking one applies; save + reload project keeps it; old project (inverted) maps to Black/White. Check legibility of red-style accents on each (Sulfur has none by design), spectrum fill, GR meter, lit buttons.
40. **SAT block layout**: HP and LP rows in one cell drag/wheel/double-click independently; TONE, SAT MIX, SAT GAIN (inverted) in place; SAT GAIN cell and the plot drag edit the same value. Output and EQ OUT GAIN cells also inverted.
41. **Lookahead**: L.AHEAD 0 -> bit-identical to before. Raise it on drums: transients are caught earlier (less overshoot), host PDC shifts the track by the same time (check alignment against a duplicate track, and Mix 50 % stays comb-free), Delta still sensible. Changing it live clicks (known).
42. **EQ bypass squares**: click the square in the EQ button / PRE tab / POST tab -> X appears, audio of that section is out (A/B with a strong boost), fades without clicks; re-enable has no burst. Module bypass overrides both; states persist in the project.
43. **Output column**: order MAKEUP, MIX, OUT GAIN (bottom, inverted). Values stay put as they change sign/size (zero padded); comp viewport metrics and OUT info lines don't jitter.
44. **Bypass dimming**: BYPASS on -> sat plot/selector/OS/EQ button/viewport/sat+comp cells at 50 %, header/out column full; off -> normal. Check on a few colour schemes.
45. **Readout refresh**: comp viewport LVL/OUT/GR and OUT column GR/PK/RMS update ~8x/s (readable), right-aligned values; IN>OUT, MAKEUP, MIX, OUT GAIN show plain values (no zero padding); SAT GAIN cell reads `+0.0dB`.
46. **EQ bypass dimming**: bypass PRE -> PRE grid dims (POST grid does not); module bypass -> viewport + grid dim; global Bypass dims as before; combos stay at 50 %.
