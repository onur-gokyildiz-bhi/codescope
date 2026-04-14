# Victor's UI Audit — 2026-04-14

> "If you can't navigate 50K nodes without it becoming a hairball, the feature isn't done."
> Reviewed: `crates/web/frontend/src/` at HEAD (`b1770a2`).

---

## Scalability

- **Cluster mode wired**: **Yes.** `api.graph(center?, depth=2, clusterMode='auto')` always sends `cluster_mode=auto` to `/api/graph` (`api.ts:20-28`). The `Graph3D` `createEffect` at `Graph3D.tsx:113-126` calls `api.graph(center || undefined, graphDepth())` — it relies on the default argument, which means auto is wired but **not user-toggleable from the UI** (no `PhysicsPanel` / `FilterPanel` control exposes it; only the backend default can override).
- **Cluster nodes render distinctly**:
  - Size: `nodeVal = 12` for `kind === 'cluster'` vs `4` for knowledge and `1-4` (s²) for code. `Graph3D.tsx:25`. Matches the "3x size" spec in agent mandate.
  - Color: `cluster: '#a371f7'` in `utils/colors.ts:21`. Matches the purple spec exactly.
  - Shape: cluster nodes do **not** get a custom `nodeThreeObject` — they render as default spheres (large purple). The octahedron is knowledge-only. Acceptable, but a distinct shape for clusters would improve scannability.
- **Hairball risk on 10K+ nodes**: ❌ partial.
  - Auto-clustering collapses the initial load, which is the right move.
  - But there is **no LOD system** (no camera-distance thresholds, no label culling, no node-count ceiling enforced client-side). The mandate specifies "Keep node count under 500 visible" — nothing enforces that beyond backend cluster decisions.
  - Hover-highlight builds a `Set<string>` by linear-scanning `data.links` on every color callback (`getConnected` at `Graph3D.tsx:134-145`). At 10K links this is called per node per frame — **O(nodes × links) per frame**. Will tank FPS above ~2K nodes.
  - Force simulation has no stop condition beyond `cooldownTicks(120)`. For big graphs the warmup is costly.

---

## Design system

- **Token adoption**: ~60%. Spacing/typography/shadow scales are defined (`styles.css:11-34`) and used extensively, but the file still carries **~50 hardcoded px values** outside the `:root` block:
  - Raw widths: `180px`, `400px`, `260px`, `320px`, `520px`, `140px`, `100px`, `200px`, `1100px`, `50px`, `40px`, `30px` — panel/viewport sizing, no token scale exists for this (fair, but should be a `--size-*` scale).
  - Raw paddings: `6px var(--space-3)`, `5px 10px`, `5px 8px`, `padding: 2px 6px`, `padding: 1px 6px`, `padding: 10px var(--space-3)` — mixing tokens with raw px inside the same declaration. Inconsistent.
  - Raw radii: `6px` (4 occurrences), `4px`, `3px`, `2px`, `10px` — the token `--radius: 8px` exists but is only used for panels. No scale for small radii. Should define `--radius-sm`, `--radius-md`, `--radius-pill`.
  - Raw font sizes in CSS: `font-size: 14px` (settings-btn, error-toast-close), `font-size: 16px` (panel-close), `font-size: 10px` (tag-pill) — **all of these break the typography scale**. `--font-xs` is 11px, `--font-xl` is 16px. No reason panel-close should be a raw 16px.
- **Inline styles in components bypass tokens entirely**:
  - `Sidebar.tsx:93,118,156`: `font-size:12px;color:var(--text-dim)` — should be `var(--font-sm)`.
  - `App.tsx:128,148`: hardcoded `bottom:8px`, `top:auto;bottom:8px;max-height:300px`.
  - `ClusterView.tsx`, `FileTree.tsx`, `ConvPanel.tsx`, `CommandPalette.tsx:138`: all use raw `font-size:12px` inline.
  - Approx. **10+ inline-style violations** of the token system.
- **Typography consistency**: Mixed. Scale is good (xs/sm/md/lg/xl), but the raw `10px` (tag-pill), `14px` (buttons), `16px` (panel-close) poke holes in it. The scale should **forbid** raw px in font-size entirely.
- **Color system**: Decent for semantic categories (kind colors, edge colors, knowledge badges). Confidence traffic-light (`#3FB950`/`#D29922`/`#F85149`) is used consistently. But these hex values are duplicated in **3 places** (`colors.ts`, `styles.css` badges, `styles.css` toast) — should be `--color-success`, `--color-warn`, `--color-danger` tokens. Cluster purple `#a371f7` lives in `colors.ts` only; not promoted to CSS.

---

## UX states

- **Loading**: ✅ mostly.
  - Global `loading` signal drives a top-of-viewport `.loading-bar` with slide animation (`App.tsx:61-63`, `styles.css:767-787`). Good.
  - Graph3D wraps its `api.graph` calls in `setLoading(true/false)` (both effects). Good.
  - Per-view components have their own local `loading` signals: `HotspotChart`, `ClusterView`, `ConvPanel`. Also good.
  - **Gap**: `App.tsx` `createEffect` at L51-57 fetches `api.stats()` with **no loading indicator and an empty `catch {}`**. If stats endpoint hangs, user sees no feedback.
  - **Gap**: `Sidebar.tsx` fetches `nodeDetail`/`knowledgeDetail` with **no spinner** — the panel just stays partially empty while loading.
  - **Gap**: `CommandPalette.tsx` debounces search but shows no "searching…" indicator during the 150ms + network RTT. On a slow link the palette looks frozen.
- **Error**: ✅ / ⚠️ inconsistent.
  - Global `errorMsg` signal drives a nicely-designed `.error-toast` with dismiss button (`App.tsx:153-158`). Nice touch: animated entry.
  - **But** only `Graph3D` actually sets it. Every other fetch swallows errors silently:
    - `App.tsx:56` — `catch { /* server may not be ready */ }`
    - `Sidebar.tsx:23,28` — `catch { /* ignore */ }`
    - `CommandPalette.tsx:62` — silent catch
    - `ConvPanel`, `HotspotChart`, `ClusterView` — per-component loading states end but no error state.
  - If `/api/stats` 500s, the header shows no stats badge and the user has zero signal anything is wrong.
  - If `knowledgeDetail` 404s (stale node), the sidebar silently renders only the header row.
- **Empty**: ⚠️ partial.
  - `FileTree`: "No files indexed" ✅
  - `HotspotChart`: "No hotspot data available" ✅
  - `ClusterView`: "No cluster data" ✅
  - `ConvPanel`: "No {tab} found" ✅
  - `CommandPalette`: "No results" ✅
  - `SourceViewer`: "No symbols" ✅
  - **Graph3D: NO empty state.** If a fresh user opens the app against an empty DB, they see a black void with no instructions. No "Index a repo to begin" CTA. This is the **first screen** most users see. Critical miss.
  - No empty state for the whole app-shell either: zero projects → `ProjectSwitcher` renders but no onboarding copy.

---

## Red flags

1. **Empty DB = black void of nothingness.** First-run UX on the default view (Graph) is a pure 3D background with nothing drawn. No hint to run `cargo run -p codescope -- index`. This is the #1 thing Bret Victor would hammer: *the interface should teach itself*. A fresh user can't tell if the app is broken or empty.
2. **`getConnected` recomputes on every hover paint.** Quadratic-ish work on link count, called from `nodeColor` and `linkColor` callbacks. For 10K-edge graphs this will stutter hard. Memoize per-hover.
3. **`window.THREE` global is a race.** `Graph3D.tsx:50-56` uses `(window as any).THREE.OctahedronGeometry(4)` but the `THREE` assignment happens asynchronously *after* `ForceGraph3D()` is already configured (`L106-109`). If the first node ingestion fires `nodeThreeObject` before `await import('three')` resolves, the try/catch silently returns `undefined` and knowledge nodes render as spheres until the next redraw. Non-deterministic visual bug.
4. **Silent error swallowing everywhere.** Seven `catch {}` blocks. Failures vanish into the ether. At minimum, log to console; ideally, route through `setErrorMsg`.
5. **Search input in header is `readOnly` and opens palette on focus.** Clever, but it's a fake input that looks editable. Users will try to type and get confused when nothing happens (onFocus fires once, palette opens, but the keystrokes are lost to the header input focus). Should be a button styled as an input, or auto-forward keystrokes to the palette.
6. **No LOD, no node-count ceiling.** Mandate says <500 visible. Nothing enforces it client-side. If backend `cluster_mode=auto` threshold is raised or broken, frontend has no backstop.
7. **Cluster expansion UX missing from what I can see.** Mandate says "Click cluster node to expand folder" — `onNodeClick` at `Graph3D.tsx:91-96` only calls `setSelectedNode` + `setCenterNode` for non-knowledge. Cluster nodes just re-center; there is no explicit expand/zoom action. Either the backend re-returns nested data (fine) or the feature is incomplete (unclear from frontend alone).
8. **Inline styles creeping in.** The design-token discipline is breaking down in components. Every `style="font-size:12px"` is a future tech-debt migration when you add a theme.
9. **Command palette does unify code + knowledge search** ✅ (`CommandPalette.tsx:44-56` branches on `isKnowledge(r.kind)`, routes knowledge → `setSelectedNode`, code → `setCenterNode`). Confidence dot + kind badge both render. This one's good.
10. **Sidebar knowledge panel is intact** ✅ — renders content, supports/contradicts/related, tags, source URL, confidence badge. Clicking a related knowledge item re-navigates via `setSelectedNode`. Solid.

---

## Action items

1. **Add a Graph3D empty state.** When `graphData().nodes.length === 0`, render an overlay with:
   - "No code indexed yet."
   - A copyable command: `cargo run -p codescope -- index <path> --repo <name>`
   - Or: "Switch project" CTA if other projects exist.
2. **Memoize `getConnected`** — recompute only when `hoveredNode()` changes, not on every color callback invocation. Stash the `Set` in a `createMemo`.
3. **Await `three` import before wiring the graph.** Move the `await import('three')` *before* `ForceGraph3D()(container)` so `window.THREE` is guaranteed available. Kills the race.
4. **Route all API errors through `setErrorMsg`.** Replace every `catch { /* ignore */ }` with `catch (e) { console.error(e); setErrorMsg('<what failed>'); }`.
5. **Expose `cluster_mode` in `PhysicsPanel` or `FilterPanel`** — a 3-way toggle (auto / off / force). Auto should stay default but power users need the escape hatch.
6. **Kill inline `font-size:12px`** — add a CSS class `.text-dim-sm` that maps to `var(--font-sm)` + `var(--text-dim)` and use it. Or, better: ban inline styles via a lint rule.
7. **Define missing token scales**:
   - `--radius-sm: 3px; --radius-md: 6px; --radius-lg: 8px; --radius-pill: 10px;`
   - `--color-success / --color-warn / --color-danger` (promote the traffic-light hexes).
   - `--size-panel-sm: 200px; --size-panel-md: 260px; --size-panel-lg: 320px;` for floating panels.
8. **Replace the fake header search** with a `<button class="header-search-trigger">` that opens the palette. Current `readOnly` input misleads users.
9. **Add per-panel loading indicator to `Sidebar`** — a skeleton or `.loading-bar` while `nodeDetail`/`knowledgeDetail` resolves.
10. **Enforce a visible-node ceiling** — if `graphData().nodes.length > 500 && cluster_mode !== 'auto'`, show a warning toast: "Rendering N nodes — enable clustering for performance."
11. **Distinguish cluster nodes visually beyond size+color** — consider a wireframe/ring shader or a label badge ("12 files") so they don't just look like oversized spheres. Seeing a *container* should feel different from seeing an *entity*.
12. **Stop lying about click target**: `onNodeClick` on a cluster node currently calls `setCenterNode(node.name)` which likely has no effect if the cluster name isn't a real entity. Either make cluster clicks expand the cluster (re-query with `cluster_mode=off` scoped to the subset) or disable the click visually.

---

*Seeing is understanding. Right now a first-time user opens the app and sees darkness. Fix that before anything else.*
