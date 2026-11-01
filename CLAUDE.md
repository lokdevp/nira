# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Nira is a comic book and manga viewer in Rust and GTK4 (`gtk4-rs` 0.9, GTK ≥ 4.12). Version 0.5 is a rebuild: the tested non-GTK code was carried over, the UI is being rewritten, and the config formats are new.

The old app (0.4.2) is the feature reference while the UI is ported. The UI follows the user's Claude Design "Comic Reader Wireframe" (artifact `b28c8b19-6499-4805-a57c-8dfffde087c8`; boards `project/Main.dc.html`, `Focus.dc.html`, `Welcome.dc.html`). That design is the spec for look and layout; the old app's `app/chrome.rs` and `style.css` were built from it and are only a GTK reference. Done: open and turn pages, fit/zoom/smart scroll, the whole design (title bar, toolbar, page bar, View panel with auto-enhance, page sidebar and bookmarks, focus mode, start screen with history, sibling books, magnifying lens), the preferences dialog with the keybinding editor, rotation, Go to page and the About dialog (opened from the title bar's app mark). Left out on purpose: a library (the design has none; the start screen's history replaces it) and the design's Scroll layout, per-file view settings and shortcuts overlay. Decide each quirk of the old app on its own merits rather than copying it.

## Layout

- `crates/nira-core/`: everything that doesn't need GTK. It must never depend on `gtk4` (`gdk-pixbuf` is allowed; the thumbnailer uses it).
- `crates/nira/`: the GTK app, binary `nira`.
- `i18n/<locale>/nira.ftl`: Fluent strings. `en-US` is complete, and other locales fall back to it id by id.
- `test/files/`: fixtures. Tests find them through `env!("CARGO_MANIFEST_DIR")/../../test/files`.
- `assets/`, `packaging/`, `debian/`: logo, Linux/Windows/AppImage/Arch packaging, `.deb` metadata. The scripts run from the workspace root.

## Commands (from the workspace root)

```bash
cargo build                          # debug build (deps at opt-level 2)
cargo run -p nira -- path/to/book.cbz  # -W debug for verbose logging; --help for options
cargo test --workspace               # all tests (inline #[cfg(test)] modules)
cargo test -p nira-core reader       # tests matching a filter
cargo clippy --workspace --all-targets
```

To look at the UI without a desktop, run it on Broadway: start `gtk4-broadwayd :5` with `XDG_RUNTIME_DIR` set, run the app with the same `XDG_RUNTIME_DIR` plus `GDK_BACKEND=broadway BROADWAY_DISPLAY=:5`, and open `http://localhost:8085`. Point `XDG_CONFIG_HOME`/`XDG_DATA_HOME`/`XDG_CACHE_HOME` at a scratch dir so the real config isn't touched. Keep `XDG_RUNTIME_DIR` short: Unix socket paths over 108 bytes are truncated, and the stray truncated file then blocks every restart with "Address already in use". Broadway screenshots can lag input by a frame or two, so check `-W debug` logs before concluding a key was ignored. Broadway sends a constant stream of synthetic pointer motion, has no real fullscreen, and its server sometimes dies on `append_node_ops: assertion failed (!reused_node->consumed)` (its frame diffing, not Nira); restart it and carry on.

## Architecture

- **`reader.rs` (core)** holds the open book, the current page and spread, the page cache and prefetch window, and the stale-result counters (`gen` per book, `pgen` per invalidation, `req` per display request, `scale_req` per rescale). The UI never decodes or scales. It calls `Reader` methods, sends `take_commands()` to the page worker, passes worker results to `on_result()`, and draws the returned `Frame` (sizes per slot, screen order, pixel updates). Navigation is ignored while the current spread is decoding. Any change that invalidates decoded pages goes through `Reader::invalidate`.
- **`workers.rs` (core)** has the page worker (decode and pre-scale, rescale), the thumbnailer, the persister and `open_archive`. Nothing there touches GTK. The app's `workers.rs` starts them and relays results onto `async_channel`s that the window awaits with `glib::spawn_future_local`; there is no polling.
- **`ordering.rs`, `zoom.rs`, `layout.rs` (core)**: spread and manga ordering (`spread_of`, `visual_order`, `step_magnitude`), the typed fit model (`FitMode`, `FitSize`), and the smart-scroll box model.
- **`archive/` (core)**: the `Archive` trait, magic-byte `detect` and `open`. ZIP and TAR are pure Rust; 7z uses the external `7z` with a `sevenz-rust` fallback; RAR, LHA and PDF shell out to `unrar`/`7z`/`lha`/`mutool`. `recursive.rs` handles archives inside archives.
- **Storage (core)**: `paths.rs` (`~/.config/nira`, `~/.local/share/nira`, `~/.cache/nira`), `settings.rs` (`settings.toml`, sections with defaults on every field), `keymap.rs` (`keys.toml`, only changed actions saved), `bookmarks.rs` and `history.rs` (JSON; changing them doesn't write, the app saves on the persistence thread), `thumb_cache.rs` (`thumbnails-v2`). `siblings.rs` finds the previous/next book; `thumbs.rs` orders thumbnail requests.
- **`window/` (app)**: `NiraWindow`, a subclassed `gtk::ApplicationWindow`. Its state lives in the `imp` struct; touch the reader only through `with_reader`, and never call widgets from inside that closure. Every command is a `win.<name>` GAction whose name is `keymap::Action::name()`. `install_accels` binds the keymap with `set_accels_for_action`, so there is no custom key dispatcher. The scrolled view isn't focusable, which keeps keys going to the accelerators. `impl NiraWindow` is split by concern: `mod.rs` (state, opening, workers, `update_chrome`, settings via `update_settings`, which saves on the persistence thread), `actions.rs`, `nav.rs`, `view.rs` (drawing, fit, zoom, layout toggles), `scroll.rs`, `controls.rs` (page bar, auto-hide, slideshow), `viewopts.rs` (View panel), `sidebar.rs` (thumbnails, bookmarks), `focus.rs`, `books.rs` (history, start screen, drag and drop, sibling books) and `about.rs`. Toggles and choices are stateful actions (`page-layout`, `direction`, `fit`, `background`, …) that widgets bind to with action targets; code changes them with `set_action_state`.
- **`chrome/` (app)**: one struct of widgets per part of the design (`titlebar`, `toolbar`, `pagebar`, `viewpanel`, `sidebar`, `statusbar`, `focus`, `welcome`) with an `update` method fed plain data. Styling is `style.css` (the design's palette), loaded by `theme.rs`, which also sets the viewer background at runtime.
- **`scroll.rs` (core)**: line-scroll steps from adjustment values, `EdgeGuard` (turn after N presses at the edge), and `SpreadLayout` (smart scroll and the start/end positions after a page turn, built on `layout.rs`).
- **`texture.rs` (app)**: textures over decoded pixels without copying, and `SizedPaintable`, which keeps a page at its display size while a differently sized texture stands in.
- **i18n**: `nira_core::i18n::t("id")` and `t_args("id", &[("name", value.into())])`. An action's label is `action-<name>`, and a test checks that every action has one.

## Release workflow

- Develop on a branch and merge into `main` with `--no-ff`. Pushing a `v*` tag runs `.github/workflows/release.yml`, which builds the .deb, tarball, AppImage and Windows zip/installer, runs `cargo test --workspace --release`, and creates a draft release.
- The version is `[workspace.package] version` in the root `Cargo.toml`. After a release is published, update `pkgver` in `packaging/arch/PKGBUILD` and run `updkgsums`.
