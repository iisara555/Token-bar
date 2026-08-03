# Token Bar

Token Bar is an always-on-top Windows toolbar for monitoring AI API spend,
token usage, balances, and budget progress across providers. The React UI runs
inside a Tauri shell; credentials stay in Windows Credential Manager and all
provider requests are made by Rust.

## UI preview

```powershell
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

Install the stable Rust toolchain, Microsoft C++ Build Tools, and WebView2,
then run:

```powershell
npm run start
```

Create the NSIS installer with:

```powershell
npm run bundle
```

The generated application icons live in `src-tauri/icons`; edit
`src-tauri/app-icon.svg` and run `npx tauri icon src-tauri/app-icon.svg` to
regenerate them.

## Installed app

Run the generated `Token Bar_0.1.0_x64-setup.exe` and launch Token Bar from the
Start menu. Open **Settings → Providers** to save API keys or use a provider's
**Link** button. Anthropic's button opens Claude.ai in Chrome; Chrome cookies
are never imported into Token Bar. The bar shows provider subscription windows
as **5H LEFT** and **WEEK LEFT** when that provider exposes them. Kimi Code can
use its official OAuth session, while Z.AI and MiniMax use defensive
plan/API-key usage adapters.
