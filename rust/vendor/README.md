# vendor/nih_plug_egui

Copy of `nih_plug_egui` from https://github.com/robbert-vdh/nih-plug (ISC, see LICENSE) with two changes:
`nih_plug` is a git dependency instead of a path dependency, and `EguiState::set_requested_size` is `pub`.

Also: `editor.rs` converts the requested logical size to physical pixels before `queue.resize` (macOS Retina resize bug).
