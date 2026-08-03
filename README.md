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
