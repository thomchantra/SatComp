# Factory presets

Every `*.json` file in this folder is embedded into the plug-in at build time and listed under FACTORY
in the preset dropdown (sorted by file name; `INIT` is always first and built in).

To make one: dial the sound in, click the save icon in the preset bar, save the file, rename it to
`NN name.json`-style if you want an order, drop it here and rebuild.

Format: `{ "format": "satcomp-preset", "version": 1, "name": "...", "params": { "<param id>": <value>, ... } }`.
Parameter ids are the ones in `src/params.rs`. Missing ids fall back to the default. Bypass and Gain Match
are never stored (they are performance switches / level-dependent).
