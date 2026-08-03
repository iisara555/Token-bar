# Token Bar

Token Bar is an always-on-top toolbar for monitoring AI API spend, token usage,
balances, and budget progress across providers. It runs on **macOS 11+** and
**Windows 10/11**. The React UI runs inside a Tauri shell; credentials stay in
the platform credential store — the login Keychain on macOS, Credential Manager
on Windows — and all provider requests are made by Rust.

## The design

The surface is black and there is no light variant. Colour in this app belongs
to the meters and to nothing else — that is what lets a glance at the bar go
straight to the one reading that changed. Provider logos render white for the
same reason: eleven brand hues would be eleven claims on the eye.

**Two ramps, and only two.**

| Ramp | Used for | Reads |
|---|---|---|
| Warm | 5-hour window, spend, credit balance | white → yellow → orange → red |
| Cool | Weekly window | white → teal → blue → purple |

Which ramp a meter wears says what *kind* of limit it is, not how full it is, so
the two never swap. Both start at pure white, which on black is the brightest
the screen can go — that puts the leading edge of an almost-empty meter at
maximum contrast. The gradient is scaled to the fill, so a meter at 40% still
ends in its ramp's full-strength colour: short, not faded.

Every meter shows what is **left**, so they all drain in the same direction. A
bar that filled up as things got worse would be the only one in the app running
backwards.

**Numerals** are set in [Bitcount Prop Single][bitcount], a dot-matrix face that
turns each reading into a small array of lit cells. It is subsetted to latin and
vendored in `src/assets/fonts` — the app's CSP allows `font-src 'self' data:`
and nothing else, so a webfont CDN would silently fail to load. Labels and prose
stay in the system UI font.

Providers only get the readings they can actually support: a subscription shows
its rate-limit windows, a pay-as-you-go account shows its balance against what
was purchased, and an account with a budget you set shows what is left of it. A
provider with nothing to report says so rather than being given an empty meter,
which would read as "zero left" instead of "no data".

[bitcount]: https://github.com/petrvanblokland/TYPETR-Bitcount

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
imported into Token Bar. The bar shows provider subscription windows as **5H**
and **W** meters when that provider exposes them. Click a provider to open its
card. Kimi Code can use its official OAuth session, while Z.AI and MiniMax use
defensive plan/API-key usage adapters.

The bar's wordmark reads **Quoken**; the product is Token Bar. The wordmark is a
logotype and does not appear in window titles, the tray, or the installer.

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
| Focus ring | System accent colour | White |

The settings window is frameless on both platforms, so it has no real macOS
traffic lights — the close control is drawn by the app and moved to the left to
match where a Mac user reaches. Adopting genuine traffic lights would mean
native decorations, which Windows cannot have without a second title bar.
