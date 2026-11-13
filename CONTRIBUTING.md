# Contributing to Langhian Translation

Thank you for helping improve **Langhian Translation**! Langhian is built in pure Rust with `iced` (zero WebView) for ultra-low memory usage, full RTL/Persian/Arabic shaping, and a modern Velvet Plum (`#685C74`) visual identity.

## Development Workflow

```powershell
# Run locally from repository root
cargo run -p langhian-app

# Run background-only / tray mode
cargo run -p langhian-app -- --minimized

# Quality gates required before any pull request
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## UI & Icon Guidelines

Langhian uses bundled SVG vector icons and does **not** load icon fonts or external CDNs at runtime:

- **UI Action & Navigation Icons** (`assets/icons/ui/*.svg`):
  - Sourced from **Google Material Symbols Rounded** (weight `400`, optical size `24`, `0 -960 960 960` viewBox / 24px grid).
  - Embedded in `crates/langhian-app/src/ui/icons.rs` via `UiIcon` enum variants (`ui_icon(UiIcon::..., size, color)`).
  - Keep SVG paths monochrome (`currentColor` / single fill path) so `iced::widget::svg::Style` can tint icons dynamically for both **Midnight Dark** and **Daylight Light** themes.
  - When adding a new icon:
    1. Place the rounded 24px Material Symbols `.svg` inside `assets/icons/ui/`.
    2. Add a `const ICON_*` byte slice and corresponding `UiIcon` variant in `crates/langhian-app/src/ui/icons.rs`.
    3. Use `ui_icon(...)` instead of Unicode emojis (`🔍`, `📌`, `⭐`, `🗑`, etc.) anywhere in interactive controls or headers.

- **Provider Brand Logos** (`assets/icons/providers/*.svg`):
  - Sourced from official brand packs / **LobeHub Icons** (`anthropic.svg`, `gemini.svg`, `google_translate.svg`, `lmstudio.svg`, `ollama.svg`, `openai.svg`, `openrouter.svg`, and `custom.svg`).
  - Registered via `ProviderLogoKind` in `crates/langhian-app/src/ui/icons.rs`.
  - Multi-color gradients (`gemini.svg`) preserve native fills (`color: None` override); monochrome marks tint smoothly to `p.text_primary` for contrast in both dark and light modes.

- **Official Application Icon** (`assets/icons/langhian.png` & `assets/icons/langhian.ico`):
  - Transparent squircle artwork in Velvet Plum (`#685C74`) and Soft Lavender (`#C9B8D4`).
  - Embedded at compile time for runtime window/tray icons (`app_logo(size)`) and Windows executable resources (`crates/langhian-app/build.rs`).

## Right-to-Left (RTL) & Localization Rules

- Whenever adding user-facing text, add matching keys to **all three** bundled locales simultaneously:
  - `assets/locales/en.json`
  - `assets/locales/fa.json`
  - `assets/locales/tr.json`
- The `langhian-core` test suite enforces 100% key parity across all locales (`all_bundled_locales_have_complete_key_parity`).
- Use `rtl_row(is_rtl, vec![...])`, `horizontal_for_dir(is_rtl)`, `font_for_ui(...)`, and `font_for_text(...)` so Persian/Arabic UI layouts flip cleanly without breaking LTR technical strings or code spans.
