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
backwards. At card size a meter also carries a printed quarter graduation below
its track — quarters, because that is the resolution a glance can use and the
numeral above already gives the exact figure.

Outside the meters there is **one accent**, `#ff6a2a`, which is the tail dot on
the app icon. It marks a control that is switched on or armed for a keystroke,
and nothing else — not status, and not categories. `--warn` and `--danger` are
kept for a reading that has left its normal range. There is deliberately no "ok"
colour: a provider reporting normally is the case that needs no signal, and
lighting it green would be one more claim on an eye that should be free to land
on the single meter that moved.

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

- `http://localhost:5183/` — floating bar
- `http://localhost:5183/popover.html` — tray popover
- `http://localhost:5183/settings.html` — settings

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

The generated application icons live in `src-tauri/icons`; edit
`src-tauri/app-icon.svg` and run `npx tauri icon src-tauri/app-icon.svg` to
regenerate them.

## Installers

`.github/workflows/build.yml` builds both installers on every push and attaches
them to the run as artifacts:

- **token-bar-windows-exe** — `Token Bar_0.1.0_x64-setup.exe` (NSIS, per-user)
- **token-bar-macos-dmg** — universal `.dmg`, runs natively on Apple silicon and
  Intel

A `.dmg` cannot be produced anywhere but macOS — Apple does not permit its SDK
to be redistributed, so there is no cross-compile from Windows. That is why the
build lives here rather than on a developer's machine.

Download them from the **Actions** tab, or:

```sh
gh run download --name token-bar-windows-exe
gh run download --name token-bar-macos-dmg
```

### Opening the macOS build

The CI build is **ad-hoc signed**, not signed with a Developer ID. Ad-hoc is
what makes it run at all — an entirely unsigned bundle is killed by the loader
on Apple silicon before Gatekeeper even offers a prompt — but it does not tell
macOS who published it, so the first launch is refused:

> "Token Bar" cannot be opened because the developer cannot be verified.

Either of these clears it, once, permanently:

- Right-click (or Control-click) **Token Bar** in Applications → **Open** →
  **Open**. The plain double-click does not offer this; the context menu does.
- Or from a terminal:

  ```sh
  xattr -dr com.apple.quarantine "/Applications/Token Bar.app"
  ```

To remove the warning for everyone instead, join the Apple Developer Program
and set these as repository secrets — the workflow switches to a real signature
and notarises automatically once they exist:

`APPLE_SIGNING_IDENTITY`, `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`

### Antigravity sign-in credentials

Antigravity's OAuth client id and secret are not committed to this repo — they
are Google's credentials for a real registered OAuth client, not this
project's to redistribute in source. Set `ANTIGRAVITY_CLIENT_ID` and
`ANTIGRAVITY_CLIENT_SECRET` as repository secrets (same mechanism as the
`APPLE_*` ones above) and the release workflow bakes them into the binary; set
them as local environment variables instead when running `cargo tauri dev`.
Without either, Antigravity's Connect button reports a config error rather
than pretending to sign in. Community reverse-engineering of the Antigravity
IDE (e.g. the `opencode-antigravity-auth` project) is one source for a working
client id/secret pair, if you want the feature to work out of the box on a
build you control; whether that redistribution is appropriate for a build you
publish is your call to make, not this project's.

### Building installers locally

```sh
npm run bundle
```

Produces a `.app` and `.dmg` on macOS and an NSIS `.exe` on Windows — each only
for the platform you are on. `src-tauri/entitlements.plist` is applied
automatically when a signing identity is present and ignored when one is not.

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

**Antigravity** is a separate Google login from a Gemini API key — it is not
another name for one, and connecting it does not touch any Gemini API key
saved elsewhere in Settings. There is no existing session to read, so its
**Connect** button in the provider card runs Token Bar's own Google sign-in
and keeps the resulting token apart from pasted API keys in the credential
store. Google publishes no usage API for Antigravity; the quota reading comes
from the same undocumented internal endpoint Antigravity itself calls and may
need updates if Google changes it. Connect needs an OAuth client id/secret
baked into the build (see [Antigravity sign-in credentials](#antigravity-sign-in-credentials));
without one it reports a config error instead of doing nothing silently.

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
| Menu bar strip | Off limits by default; **Allow in the menu bar** opens it | Taskbar is never covered |

### The menu bar and the notch

By default the bar is held below whatever the OS reserves at the top of the
display, measured from the monitor's work area rather than assumed — the menu
bar on macOS, a top-docked taskbar on Windows.

**Settings → Appearance → Allow in the menu bar** (macOS only) lets it move up
into that strip. On a Mac with a camera housing, the bar then keeps itself to
one side of the notch instead of disappearing behind it: a menu bar taller than
32pt is what identifies a notched display, and the middle 14% of the width is
treated as occupied. Turning the setting back off pulls the bar down again
rather than leaving it stranded over the clock.

The settings window is frameless on both platforms, so it has no real macOS
traffic lights — the close control is drawn by the app and moved to the left to
match where a Mac user reaches. Adopting genuine traffic lights would mean
native decorations, which Windows cannot have without a second title bar.
