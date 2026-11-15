# SatComp patches to baseview (rev 9a0b42c09d712777b2edb4c5e0cb6baf21e988f0)

Vendored through `[patch]` in `rust/Cargo.toml`. Linux / X11 only (`src/x11/window.rs`, `src/gl/x11.rs`); macOS and Windows are untouched.

1. `WindowHandle::close` waits (max 1.5 s) for the window thread to finish. Upstream only set a flag (its own FIXME), the host then destroyed
   the parent X window while the thread was still drawing, and Mesa's DRI3 code waited forever for a Present event of a drawable that no
   longer existed, holding the process-wide libGLX lock (frozen REAPER UI with several plug-ins loaded, 2026-11-15).
2. `open_parented` waits at most 4 s for the window thread (upstream: forever).
3. `Drop for WindowInner` destroys the GLX context while the display connection is still open, and `GlContext::drop` releases and destroys it
   (upstream leaked one context per editor open).
4. `swap_buffers` no longer panics the window thread on an X error of a drawable the host just destroyed.

When updating baseview, re-apply these or check whether upstream fixed the FIXME in `WindowHandle::close`.
