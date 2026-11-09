# SatComp

Saturator + compressor + EQ with a black/white/red monospace UI.

**Status:** early. The **Rust VST3/CLAP plug-in (`rust/`) is the production build**: full saturator + compressor + EQ, presets, custom waveshaper, 20 colour schemes, six Drums factory presets. `SatComp.jsfx` is the archival in-house prototype (feature-frozen at v0.3, kept for reference and A/B; new features land in the Rust build only).

## Install (JSFX, archival)
Copy `SatComp.jsfx` into REAPER's `Effects/` folder (Options → Show REAPER resource path).

## Build (VST3 / CLAP)
Requires [Rust](https://rustup.rs).

```bash
cd rust
cargo xtask bundle satcomp --release
```

Bundles land in `rust/target/bundled/`. Copy to:

| | macOS | Windows |
|---|---|---|
| VST3 | `~/Library/Audio/Plug-Ins/VST3/` | `C:\Program Files\Common Files\VST3\` |
| CLAP | `~/Library/Audio/Plug-Ins/CLAP/` | `C:\Program Files\Common Files\CLAP\` |

## Docs
`docs/PARAMETERS.md` (parameter reference), `docs/HANDOFF.md` (design decisions), `docs/TEST_CHECKLIST.md`.

## License
GPL-3.0-or-later.
