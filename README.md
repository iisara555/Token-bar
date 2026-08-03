# Token Bar

Token Bar is an always-on-top toolbar for monitoring AI API spend, token usage,
balances, and budget progress across providers. It runs on **macOS 11+** and
**Windows 10/11**. The React UI runs inside a Tauri shell; credentials stay in
the platform credential store — the login Keychain on macOS, Credential Manager
on Windows — and all provider requests are made by Rust.

## The status rail

Along the foot of the bar is a hairline that fills white → orange as the
tightest ceiling you are tracking fills up. It reads the **worst** of every
budget and every rate-limit window across all enabled providers, so it is never
averaging a nearly-exhausted weekly quota into something reassuring.

It is meant to be read without being looked at, so it signals with *length*
first and lets colour ride along: the ramp is painted across the whole rail and
revealed to the fill width, which makes the colour of the leading edge the
reading. Pale at 20%, deep orange at 100%. A notch marks the warning threshold.
It shows nothing when nothing has a ceiling — set a budget in
**Settings → Providers** to give it something to watch.

## UI preview

```sh
npm install
npm run dev
```

Open these pages while Vite is running:

- `http://127.0.0.1:5183/` — floating bar
- `http://127.0.0.1:5183/popover.html` — tray popover
- `http://127.0.0.1:5183/settings.html` — settings

Outside Tauri, the UI automatically uses seeded, session-scoped demo data. This
makes visual development safe and does not require provider keys.

## Desktop development

Install the stable Rust toolchain, then the platform prerequisites:

- **macOS** — Xcode Command Line Tools (`xcode-select --install`).
- **Windows** — Microsoft C++ Build Tools and WebView2.

Then run:

```sh
npm run start
```

Build the installers with:

```sh
npm run bundle
```

This produces a `.app` and `.dmg` on macOS and an NSIS `.exe` on Windows.

The generated application icons live in `src-tauri/icons`; edit
`src-tauri/app-icon.svg` and run `npx tauri icon src-tauri/app-icon.svg` to
regenerate them.

### Shipping a macOS build

Local builds are unsigned and Gatekeeper will refuse them on another machine.
For distribution, set `APPLE_SIGNING_IDENTITY` (plus the notarisation
credentials) before `npm run bundle`; `src-tauri/entitlements.plist` is applied
automatically when an identity is present and is ignored when one is not.

Two files exist only for macOS packaging: `entitlements.plist` (JIT for
WKWebView, outbound network) and `Info.plist` (`LSUIElement`, so no Dock icon
ever appears).

## Installed app

On macOS, drag Token Bar to Applications and launch it — there is no Dock icon
by design; it lives in the menu bar. On Windows, run the generated
`Token Bar_0.1.0_x64-setup.exe` and launch it from the Start menu.

Open **Settings → Providers** to save API keys or use a provider's **Link**
button. Anthropic's button opens Claude.ai in Chrome; Chrome cookies are never
imported into Token Bar. The bar shows provider subscription windows as
**5H LEFT** and **WEEK LEFT** when that provider exposes them. Kimi Code can use
its official OAuth session, while Z.AI and MiniMax use defensive plan/API-key
usage adapters.

### Platform behaviour

Most of the UI is identical on both platforms. These differ because the
platform convention differs, not because the design does:

| | macOS | Windows |
|---|---|---|
| Backdrop | `NSVisualEffectView` (`vibrancy`) | DWM Mica/Acrylic (`native`) |
| Menu bar / tray icon | Template image, badges by **shape** so AppKit can re-tint it | Coloured rounded square |
| App presence | Menu bar only — no Dock icon, no Cmd-Tab | Tray icon, hidden from taskbar |
| Settings close button | Top-left, plus <kbd>Cmd</kbd>+<kbd>W</kbd> | Top-right, plus <kbd>Esc</kbd> |
| Scrollbars | Native overlay, as the user configured them | Custom thin scrollbar |
| Transparency off | Accessibility → Display → Reduce transparency | Personalization → Colors |

The settings window is frameless on both platforms, so it has no real macOS
traffic lights — the close control is drawn by the app and moved to the left to
match where a Mac user reaches. Adopting genuine traffic lights would mean
native decorations, which Windows cannot have without a second title bar.
