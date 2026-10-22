## Mahgen tile sizing — what works and what doesn't

The `<mah-gen>` web component has a frustrating sizing model. We learned this
the hard way; everything below is required reading before touching tile size
code in `js/app.js`.

### What `mah-gen` actually does

Source: [`eric200203/mahgen`](https://github.com/eric200203/mahgen) `src/MahgenElement.ts`. The element constructor:

```ts
constructor() {
  super();
  const root = this.attachShadow({ mode: 'open' });
  this.img = document.createElement('img');
  root.appendChild(this.img);
}
```

A single `<img>` is appended to an open shadow root. `attributeChangedCallback`
on `data-seq` calls `Mahgen.render(seq, river)` which produces a **single
base64 PNG** (composited by `JimpWorker.ts`) and assigns it to `this.img.src`.
The PNG load is async.

### Why every "obvious" approach fails

1. **`zoom: 0.x` on the host.** Doesn't apply reliably in WebKitGTK (the Tauri
   webview on Linux). Even when it works in Chromium, it's not portable.

4. **`max-width: 100%` on the inner `<img>` (set via shadow injection).**
   Circular dependency:
   - `<img>`'s containing block resolves to the shadow root.
   - Shadow root inherits the host's content box.
   - Host (`mah-gen`, `display: inline-block`) sizes to its content (= the img).
   - So `100%` resolves to the img's own width — no constraint.

5. **`img.style.width = "100%"` (set via shadow injection).** Same circular
   problem as above.

6. **`transform: scale()` on the host.** Scales visually but doesn't change
   the layout box, so siblings don't reflow and tiles overlap.

### The approach that works

Set explicit **pixel** dimensions on **both** the host element AND the inner
`<img>`. Setting the host's box to a fixed size breaks the circularity, and
explicit pixel dims on the img bypass the percentage-resolution issue entirely.

```js
// Open shadow → reachable from JS even though CSS can't penetrate.
const img = el.shadowRoot.querySelector('img');
el.style.width = w + 'px';
el.style.height = h + 'px';
img.style.width = w + 'px';
img.style.height = h + 'px';
img.style.objectFit = 'contain';
img.style.display = 'block';
```

### Native tile dimensions (mahgen `res/` assets)

```
Normal portrait tile:    70 × 100   (e.g. 1m.png)
Horizontal (sideways):   92 ×  77   (e.g. _1m.png — used for chi/pon/riichi-cut)
Stacked (called):        92 × 146   (e.g. =1m.png — used for ankan back tile)
Space:                  ~70 × ...   (used as filler in melds)
```

### River-mode layout

The upstream mahgen `JimpWorker.ts` lays at most 6 tiles per row in river
mode, then wraps to a new row. So a river image's natural dimensions:

```
N tiles, N <= 6:     naturalW = N × 70,    naturalH = 100
N tiles, 7..12:      naturalW = 420,       naturalH = 200
N tiles, 13..18:     naturalW = 420,       naturalH = 300
...
```

This matters because aspect-fit-to-cw (`h = cw × naturalH / naturalW`) gives
**different per-tile sizes** depending on row count: 6 tiles in 1 row would
fill width with tile_h ≈ cw/6 × 1.43, but 7 tiles in 2 rows would compute
tile_h ≈ cw/12 × 1.43 (half the size). Tiles visibly shrink the moment a
second row appears. Bad.

The fix: **scale uniformly** so 6 tiles always span container width, and rows
just stack vertically:

```js
const RIVER_FULL_ROW_W = 420;    // 6 × 70
const scale = cw / RIVER_FULL_ROW_W;
const w = naturalW * scale;
const h = naturalH * scale;
```

Per-tile size stays constant regardless of row count.

### Sizing modes (current `SIZE_CTX` in `js/app.js`)

| Mode | Formula | When to use |
|---|---|---|
| `river` | `scale = cw / 420`; `w = nw * scale`; `h = nh * scale` | River — single mahgen, multi-row, must keep per-tile size constant |
| `fit` | `w = cw; h = cw * nh / nw` (clamped to `[min, max]`) | Self hand — single mahgen, single row, fill container width |
| `linear` | `h = base * cw / ref` (clamped); `w = h * nw / nh` | Multiple mahgens in same row (melds), or rec/dora where container is much wider than image |
| `fixed` | `h = base`; `w = h * nw / nh` | Top-bar dora pill — container is content-sized so any container-based formula self-feeds |

### Async timing

Three things are async:

1. **Custom element upgrade.** `customElements.define('mah-gen', ...)` runs
   when the mahgen UMD script executes. If you create `<mah-gen>` before that
   completes, `el.shadowRoot` is `null`. → RAF retry until ready.

2. **DOM connect.** `registerMahgen` is called inside `buildPanel` *before*
   the panel is appended to the DOM, so `el.isConnected` is `false` on the
   first sizing pass. → Retry with a counter (we use up to 6 RAF), then drop
   the entry. Don't delete on first detach — that was the bug that kept
   river tiles at native size for several iterations.

3. **Image load.** `img.src = base64` triggers an async decode. `naturalWidth`
   / `naturalHeight` read 0 until the `load` event fires. → Attach a single
   `load` listener that recomputes size on every src swap (mahgen swaps src
   each `data-seq` change).

```js
if (!img._mokoraOnLoad) {
  img._mokoraOnLoad = true;
  img.addEventListener('load', () => applyMahgenSize(el));
}
```

### Container resize

A `ResizeObserver` watches the size container of every registered mahgen so
that:
- Window resize cascades to all tiles
- The rail-drag handle (which changes the players column width) reflows river
  and meld tiles automatically
- The bottom-bar collapse doesn't matter (cards are flex children) but rail
  width changes do

Each container is observed once (RO calls are idempotent). The callback finds
all registry entries whose `container` matches the resized element and
re-applies size for each.

### Empty sequences

When `data-seq` is unset or `""`, the host has explicit pixel dims but the
inner `<img>` has no `src` (mahgen returns early in `genImage` for `seq===null`,
or sets `img.src = ''` on parse error). The browser renders this as an empty
sized box that looks like a gray placeholder rectangle.

Fix: `el.style.display = 'none'` when the sequence is empty. `setMahgenSeq`
unsets it on the next non-empty sequence:

```js
const seq = el.getAttribute('data-seq');
if (!seq) { el.style.display = 'none'; return; }
el.style.display = '';   // restore CSS rule's `display: inline-block`
```

### Cleanup

The retry counter handles "element will be attached momentarily" but it does
**not** handle "element was deliberately removed via `innerHTML = ''`". For
those code paths (rec list rebuild on every `analysis-result`), call
`unregisterMahgen(m)` for each `<mah-gen>` before the wipe:

```js
recList.querySelectorAll('mah-gen').forEach((m) => unregisterMahgen(m));
recList.innerHTML = '';
```

Otherwise the registry leaks an entry per old recommendation tile every time
the analysis updates.

### Quick checklist for adding a new mahgen-bearing widget

1. Pick a sizing mode (`river` / `fit` / `linear` / `fixed`) and add an entry
   to `SIZE_CTX` if needed.
2. Pick a sizing **container** — an ancestor whose `clientWidth` is what
   bounds the tiles. Don't pick a content-sized element (its width depends on
   the image, which causes self-feeding loops).
3. Call `registerMahgen(el, kind, container)` right after appending the
   `<mah-gen>` to its parent.
4. If your widget rebuilds via `innerHTML = ''`, call `unregisterMahgen` on
   each old `<mah-gen>` before the wipe.
5. Use `setMahgenSeq(el, seq)` (not `setAttribute('data-seq', ...)` directly)
   to get the opacity crossfade and the post-load size re-apply.
