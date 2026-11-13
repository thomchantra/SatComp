# Factory presets

Every `*.json` file in this folder is embedded into the plug-in at build time and listed under FACTORY
in the preset dropdown (sorted by file name; `INIT` is always first and built in).

To make one: dial the sound in, click the save icon in the preset bar, save the file, rename the copy to
`NN name.json`-style if you want an order, drop it here and rebuild.

Format: `{ "format": "satxcomp-preset", "version": 1, "name": "...", "params": { "<param id>": <value>, ... },
"curves": [[[x, y], ...], [...], [...]] }`. Parameter ids are the ones in `src/params.rs`; missing ids fall back to
the default. `curves` (optional) holds the custom waveshaper curve of the low, mid and high band; absent = linear.
Bypass and Gain Match are never stored (performance switches). The Sidechain switch (`sc_on`) is stored when a preset is saved but only applied when the preset has the key, so a sidechain preset comes back keyed while older presets and INIT leave the switch alone (it depends on the track's routing).
