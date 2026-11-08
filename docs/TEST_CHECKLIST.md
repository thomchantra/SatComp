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
