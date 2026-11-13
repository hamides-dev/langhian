# Langhian Translation

**Langhian** is an ultra-lightweight, pure-Rust native desktop translation daemon built with zero WebView overhead (`iced` + `wgpu`/`tiny-skia`), featuring a clean frameless **Midnight Dark / Daylight Light** Velvet Plum interface, global double-tap workflows, multi-provider AI routing with live per-1M pricing discovery, free Google Translate support, vision-gated screenshot translation, and saved vocabulary highlighting.

---

## Highlights

- **Zero WebView / Ultra-Lightweight**: Built 100% in native Rust (`iced 0.14` daemon). Background tray mode closes inactive surfaces so idle memory and CPU usage stay minimal.
- **First-Run Quick Setup Wizard**: Guides new users through interface language/RTL orientation, core shortcuts, and **Native Language vs. Target Language** routing (re-accessible anytime from Settings).
- **Smart Bidirectional Language Routing**: Translates foreign text into your **Native Language**, automatically detects when you type in your Native Language inside the modal/dashboard (`>= 70%` threshold) to reverse into your **Target Language**, and always locks **Double-Tap `Alt`** in-place replacement strictly toward your Target Language.
- **Provider-First Model Setup & Free Google Translate**:
  - **Google Translate (Free)** out of the box (no API key needed)
  - **OpenRouter**, **OpenAI**, **Anthropic Claude**, **Google Gemini**
  - **Local LLMs**: **Ollama** (`localhost:11434/v1`), **LM Studio** (`localhost:1234/v1`), or **any custom OpenAI-compatible endpoint**
  - Searchable provider catalogs displaying per-1M input/output pricing, context length, and vision capability badges; API keys stored safely in the OS Credential Store (`Windows Credential Manager` / `macOS Keychain` / `Linux Secret Service`).
- **Global Double-Tap Workflows**:
  - **Double-Tap `Alt`** — Translate selected text in any input field and replace it in place (with optional auto-`Ctrl+A` fallback and clipboard restoration).
  - **Double-Tap `Ctrl`** — Open a focused floating translation modal anchored right near your selection (or centered if out of bounds; remembers dragged screen coordinates). Click outside to dismiss or pin with one click.
  - **Screenshot Region Translation** (`Ctrl+Shift+X`) — Freeze-frame region snipping with vision-capability enforcement (explains clearly and offers 1-click model switch if the active model is text-only).
- **Markdown-Aware Translation & Vocabulary Star Highlighting**:
  - Preserves headings, bullet/numbered lists, blockquotes, and code spans when copying rich web/document selections, and lets you star/unstar translations directly into your searchable Vocabulary book with clickable inline highlights.
- **Custom Prompts & Instant Shortcut Switching**:
  - Define unlimited custom prompts (`{text}`, `{source_lang}`, `{target_lang}`) with direct quick-switch slots (`Ctrl+Alt+1..9`) and cycle hotkeys (`Ctrl+Alt+Left` / `Ctrl+Alt+Right`).
- **Crisp Bundled Typography & Vector Iconography**:
  - Bundled `Inter` and `Vazirmatn` fonts with Unicode BiDi + HarfRust shaping and automatic RTL UI flipping (**English `en`**, **Persian `fa` RTL**, **Turkish `tr`**).
  - Custom Velvet Plum (`#685C74`) application icon paired with bundled **Material Symbols Rounded** UI icons and **LobeHub** provider brand SVGs (see [CONTRIBUTING.md](CONTRIBUTING.md)).
- **Instant Auto-Save Settings & In-App Updates**:
  - Clean sub-tabbed Settings where changes apply and save automatically, plus `version.json` / GitHub Releases update checking and installer launch.

---

## Default Shortcuts

| Trigger | Action |
| :--- | :--- |
| **Double-Tap `Alt`** | Translate selected text in active input & replace in place |
| **Double-Tap `Ctrl`** | Open floating translation modal for selected text |
| **`Ctrl + Shift + X`** | Screenshot region translate |
| **`Ctrl + Alt + 1..9`** | Switch directly to prompt slot 1–9 |
| **`Ctrl + Alt + Right / Left`** | Cycle next / previous prompt preset |
| **`Ctrl + Shift + L`** | Open Langhian dashboard |
| **`Esc`** | Close modal or cancel screenshot snipping |

All triggers and timings are customizable in **Settings**.

---

## Platform Notes

- **Windows**: Runs natively out of the box (`WH_KEYBOARD_LL` ignores synthetic keystrokes).
- **macOS**: Grant **Accessibility** (for global hooks & paste automation) and **Screen Recording** (for screenshot capture) under *System Settings → Privacy & Security*.
- **Linux**: Full support on **X11**. On Wayland sessions, global modifier hooks and synthetic paste depend on compositor support (`wlr-data-control`, `libei`, or running under XWayland).

---

## Building from Source

### Prerequisites
- Rust toolchain (`1.88+`, edition `2024`)
- On Linux (Ubuntu/Debian):
  ```bash
  sudo apt-get update && sudo apt-get install -y \
    libgtk-3-dev libayatana-appindicator3-dev \
    libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libx11-dev libxdo-dev libxtst-dev libxi-dev libxkbcommon-dev \
    libwayland-dev libpipewire-0.3-dev pkg-config
  ```

### Build & Run
```bash
cargo test --workspace
cargo build --release
```

---

## License

MIT License — see [LICENSE](LICENSE) and [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
