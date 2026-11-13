# Langhian Translation

**Langhian** is an ultra-lightweight, pure-Rust native desktop translation daemon built with zero WebView overhead (`iced` + `wgpu`/`tiny-skia`), featuring an Antigravity-inspired minimalist interface, global double-tap workflows, multi-provider AI routing, free Google Translate support, vision-gated screenshot translation, and saved vocabulary highlighting.

---

## Highlights

- **Zero WebView / Ultra-Lightweight**: Built 100% in native Rust (`iced 0.14` daemon). Background tray mode closes inactive surfaces so idle memory and CPU usage stay minimal.
- **Any AI Provider + Free Google Translate**:
  - **Google Translate (Free)** out of the box (no API key needed)
  - **OpenAI**, **OpenRouter**, **Anthropic Claude**, **Google Gemini**
  - **Local LLMs**: **Ollama** (`localhost:11434/v1`), **LM Studio** (`localhost:1234/v1`), or **any custom OpenAI-compatible endpoint**
  - API keys stored safely in the OS Credential Store (`Windows Credential Manager` / `macOS Keychain` / `Linux Secret Service`).
- **Global Double-Tap Workflows**:
  - **Double-Tap `Alt`** — Translate selected text in any input field and replace it in place (with optional auto-`Ctrl+A` fallback and clipboard restoration).
  - **Double-Tap `Ctrl`** — Translate selected text and display it inside an `AlwaysOnTop` floating modal.
  - **Screenshot Region Translation** (`Ctrl+Shift+X`) — Freeze-frame region snipping with vision-capability enforcement (explains clearly and offers 1-click model switch if the active model is text-only).
- **Custom Prompts & Instant Shortcut Switching**:
  - Define unlimited custom prompts (`{text}`, `{source_lang}`, `{target_lang}`) with direct quick-switch slots (`Ctrl+Alt+1..9`) and cycle hotkeys (`Ctrl+Alt+Left` / `Ctrl+Alt+Right`).
- **Vocabulary Saving & Highlighting**:
  - Save words or sentences from translations; future translations automatically highlight matching vocabulary with clickable inline previews.
- **First-Class RTL & Multi-Language UI**:
  - Bundled `Inter` and `Vazirmatn` fonts with Unicode BiDi + HarfRust complex shaping for crisp Persian/Arabic translations.
  - Multi-language UI powered by JSON locale files: **English (`en`)**, **Persian (`fa`, RTL)**, and **Turkish (`tr`)**.
- **In-App Updates & Native Installers**:
  - Checks `version.json` / GitHub Releases directly from Settings and downloads/launches platform installers.

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
