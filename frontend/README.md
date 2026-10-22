# frontend (React + Vite + react-grid-layout + shadcn/ui)

Modular rebuild of the Mokora V3 frontend. The legacy vanilla-JS UI under
`../frontend/` keeps working until cutover (see "Cutover" below).

## Stack

- Vite + React 19 + TypeScript
- Tailwind CSS v4 + shadcn/ui (Radix Nova preset)
- react-grid-layout 2.x — draggable / resizable dashboard
- react-router-dom (HashRouter) — routes for Overview / Game / Bots / Logs / Settings
- zustand — app state, sliced by domain (game, analysis, bot, proxy, notify, layout, config)
- react-i18next — `en` / `zh-TW` / `zh-CN` / `ja`
- @tauri-apps/api — invoke + listen wrappers (`src/lib/tauri.ts`)
- mahgen — `<mah-gen>` web component, wrapped by `src/components/Mahgen.tsx`

## Layout

```
src/
  App.tsx                    Sidebar + Routes
  main.tsx                   HashRouter + i18n init
  index.css                  Tailwind v4 + theme tokens + RGL skin
  components/
    Sidebar.tsx              Left nav, brand, language selector
    Statusbar.tsx            Bottom strip
    TileFrame.tsx            Card + drag handle + close (×) button
    AddTileMenu.tsx          Restore hidden tiles
    Mahgen.tsx               <mah-gen> React wrapper
    ui/                      shadcn primitives (copy-pasted; edit freely)
  routes/
    Overview.tsx             Status summary
    GameDashboard.tsx        Responsive dashboard
    Bots.tsx                 Table + install + per-bot settings drawer
    Logs.tsx                 Active session + open folder
    Settings.tsx             AppConfig form
  tiles/
    defaults.ts              DEFAULT_LAYOUTS per breakpoint, TILE_TITLES
    registry.tsx             id → component
    HeaderTile.tsx … ProxyControlTile.tsx
  stores/
    gameStore.ts             GameStateSnapshot + MahgenView
    analysisStore.ts         AnalysisResult
    botStore.ts              BotStatus + BotInfo[]
    proxyStore.ts            ProxyStatus
    notifyStore.ts           events / responses / toasts (capped, FIFO)
    configStore.ts           AppConfig + log dir
    layoutStore.ts           RGL layouts + hidden tiles, persisted to localStorage
  hooks/
    useTauriBridge.ts        One-shot Tauri event subscription, mounted from <App>
  i18n/
    index.ts                 i18next bootstrap
    resources/{en,zh-TW,zh-CN,ja}.json
  lib/
    tauri.ts                 invoke + listen wrappers
    format.ts                fmtScore / fmtTime / pct / kyokuLabel
    tileIdx.ts               mjai tile <-> 0..33 index, mjaiToMahgen
    mahgenRegistry.ts        port of frontend/js/app.js mahgen sizing
    utils.ts                 shadcn `cn`
  types.ts                   Schema mirror (MjaiEvent, AnalysisResult, …)
```

## Tile catalog

14 tiles wired to stores. Each is wrapped by `<TileFrame>` (shadcn `Card`
with a `.tile-drag-handle` header and `×` close button). Hidden tiles are
restored via the `<AddTileMenu>` dropdown in the dashboard toolbar; "Reset
Layout" reverts to defaults stored in `tiles/defaults.ts`.

## Mahgen

`<mah-gen>` is rendered via the React wrapper at `src/components/Mahgen.tsx`,
which delegates to a registry-singleton in `src/lib/mahgenRegistry.ts` (a
faithful port of `../frontend/js/app.js`). The async upgrade / DOM-connect /
img-load handshake is kept verbatim — see `../frontend/mahgen.md` for why.

## Tauri integration

The backend contract (commands + events) is unchanged. v2 calls the same
IPC surface as v1, listed in `../frontend/README.md`. `useTauriBridge` is
the one place that subscribes — components read state via zustand selector
hooks (`useGameStore(s => s.view)`).

## Scripts

```
npm install             # one-time
npm run dev             # Vite dev server on :1420 (intended for `tauri dev`)
npm run build           # tsc -b && vite build → dist/
npm run preview         # serve dist/ for sanity check
npm run lint            # eslint
```

## Cutover (when v2 reaches parity)

Edit `../tauri.conf.json`:

```jsonc
{
  "build": {
    "frontendDist": "./frontend/dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "cd frontend && npm run dev",
    "beforeBuildCommand": "cd frontend && npm run build"
  }
}
```

Caveats before flipping:

1. `tauri.conf.json` defines a second window with `url: "debug.html"`. The
   v2 build does not contain a `debug.html`. Either: (a) drop the debug
   window from the conf, (b) keep v1 and only switch later, or (c) port
   the debug page to v2 (`src/routes/Debug.tsx`).
2. On Linux the WebKitGTK webview ignores some CSS quirks — exercise the
   game view with a live game and compare against `../frontend/` before
   shipping.
3. Keep `../frontend/` in tree for at least one release as a rollback path.

## i18n

Strings live in `src/i18n/resources/*.json`. Sidebar nav and tile titles
use `t(...)` already; remaining hard-coded English strings inside
individual tiles are placeholder copy and should migrate as parity testing
surfaces them.

## Known gaps (deferred)

- Log file tail viewer (Logs page is list-only)
- Multiple saved layouts ("play" / "analyze" / "spectate")
- Mobile-first layout
- Full a11y audit
- Replacing the `mahgen` UMD CDN script with the npm package import
- Dropping the chunk-size warning via code splitting
