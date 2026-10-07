# E2E coverage map (browser-only behaviour)

Playwright specs in `e2e/` run against the Vite dev server with the Tauri IPC mock in `e2e/fixtures/tauriMock.ts`.
Specs in this file's scope cover behaviour that needs a real browser (DOM, canvas, pointer events, file inputs, SVG).
Pure logic lives in `unit/` (see `docs/UNIT_TEST_MATRIX.md`).

Run: `bunx playwright test` (use `--workers=5` on a busy machine; the dev server is shared by all workers).

## Fixture additions (`e2e/fixtures/tauriMock.ts`, additive)

- `seed[].children`: pre-populate a multi/toggle action with child instances; `create_instance` into a slot that has children appends a child, `remove_instance` of a child context removes only that child.
- `tauri.fail(cmd, message | null)`: make every invoke of `cmd` reject.
- Startup image project is stateful (`get/save_startup_image_project`).
- `mockState` mutations for native dialogs (`dialogAsk`, `dialogOpen`) and `buildInfo`. `ask()` is a `plugin:dialog|message` call with `buttons: "YesNo"`.

`e2e/lib/support.ts` installs `window.__t` canvas helpers (solid PNGs, pixel readers, bounding boxes) and `openForModules(page)`, which opens the app so that `import("/src/lib/<file>.ts")` works inside `page.evaluate`.

## src/lib (`e2e/lib/`)

| Function / export | Spec | Notes |
| --- | --- | --- |
| `rendererHelper.getImage` | `rendererHelper.pw.ts` | alert fallback, fallback resolution, `opendeck/` mapping, plugin web-server URLs, every base64 type, empty payload, SVG normalisation |
| `rendererHelper.CanvasLock` | `rendererHelper.pw.ts` | serialisation, ordering, re-acquire |
| `rendererHelper.renderImage` (+ internal `drawBaseImage`, `drawText`, `drawUnderline`, `drawOverlay`, `shrinkForPress`, `loadImage` cache) | `rendererHelper.pw.ts` | pixel assertions: image fill, fallback, `processImage`, `sourceImage`, offscreen canvas, text on/off, alignment, colour/outline, size, multi-line, underline, bold/italic, canvas scale, OK/alert overlays, pressed margin, decode cache + GIF bypass + 256 eviction + failed decode not cached |
| `rendererHelper.resizeImage` | `rendererHelper.pw.ts` | GIF passthrough, 288x288 PNG, square/wide/tall letterboxing |
| `portal.portalToBody` / `portalToPreviewDock` | `portal.pw.ts` | move, destroy, clipping ancestor escape, missing dock |
| `startupImage.decodeImage`, `getFittedImageSize`, `getTransformBounds`, `rotateVector`, `drawComposedImage` | `startupImage.pw.ts` | real decoded images and canvas pixels (cover fit, zoom, offset, order, rotation, undecoded skip, resize, zero output) |
| `startupImage.readFile` (+ SVG sanitising) | `startupImage.pw.ts` | mime normalisation, DOMPurify stripping (script, handlers, foreignObject, iframe/object/embed, javascript:), invalid SVG errors, >8 KiB chunked base64 |
| `startupImage.partitionFiles`, `makeLayerId`, constants | `startupImage.pw.ts` | extension/mime/size rules, UUID and fallback ids, AKP05 mask geometry |
| `startup.showStartupTask` / `completeStartupTask` | `startup.pw.ts` | real `#startup-loader` lifecycle (status messages, leaving class, removal) |

Not browser-bound (left to `unit/`): `Context`, `DeviceFrameCoordinator`, `applicationProfiles`, `deviceFrames`, `imageFormat`, `plugins`, `ports`, `profileFolders`, `profileRendering`, `propertyInspector`, `settings`, `shims`, `singletons`.
Several of these are also exercised indirectly by the component specs (frame batching in `keyRendering.pw.ts`, `getFetch`/`releasesEndpoint` in `pluginStore.pw.ts`, `shims` `open()` in `pluginStore.pw.ts`).

## Components (`e2e/components/`)

| Component | Spec | Behaviour covered |
| --- | --- | --- |
| `NoDevicesDetected` | `noDevicesDetected.pw.ts` | empty state, title/badge, udev hint by platform, restart invoke, device hot-plug |
| `ParentActionView` | `parentActionView.pw.ts` | multi and toggle variants, step/state labels, selection (click, Enter/Space, background), Escape/back, remove child, drop zone (create_instance context, multi-action support rule, toggle nesting rule, empty payload), highlight, counts |
| `Key` (+ `LoadingSquares`) | `keyRendering.pw.ts` | canvas formats, painted vs empty, selection outline, ctrl+click, initial 14-frame batch, pressed state, `show_ok`/`show_alert`, `update_state` (replace and null), listener registration, GIF animation + stop on delete, static image no loop, loading animation (in flight, no icon), encoder zones, 4x8 `sd-` size |
| `KeyContextMenu` | `keyContextMenuActions.pw.ts` | portal, items, single menu, closing, delete/copy/paste invoke args (keys and encoders), paste without copy, cross-controller paste, edit target |
| `InstanceEditor` | `instanceEditor.pw.ts` | open/close paths, state switch, show text, text, bold/italic/underline, family + datalist, colour, alignment, size, choose image (resize, GIF), solid colour, reset, right-click reset, live key re-render |
| `DeviceStartupImage` + `StartupImageLayerList` + `StartupImageMask` | `startupImageEditor.pw.ts` | load/empty/loading/error states, mask, add (single, multi, SVG), validation errors (type, mime, size, corrupt, invalid SVG, script stripping, 64 limit), select/remove/reorder, drag/resize/rotate/reset, apply (order, args, JPEG output, busy, failures, retry), reload |
| `ReleaseAssetChooser` | `pluginStore.pw.ts` | filtered asset list, default/selected asset, cancel, Escape priority, reopen |
| `PluginDetails` | `pluginStore.pw.ts` | README fetch order/fallbacks, markdown sanitising, link rewriting, download count, author link, release button, close paths |
| `PluginManager` install flows | `pluginStore.pw.ts` | install from catalogue, decline, failures, archive, from file, deep link |
| `ApplicationProfilesPopup` + `ProfileOptions` | `applicationProfilesPopup.pw.ts` | add (both orders), default profile, sorting, change, remove, persistence args, error banner, existing mappings, other devices, optgroups, `applications` event, deleted-profile cleanup |
| `Popup` / `PopupHeader` / `ListedPlugin` / `Tooltip` | `popups.pw.ts` | full-screen vs inline variants, header variants, backdrop/close, developer-mode reload, dimming, tooltip hover |

Tests that already existed (`e2e/*.pw.ts`) are unchanged except the two stale `"OpenDeck"` title assertions (`layout.pw.ts`, `deviceSelector.pw.ts`) that no longer matched the working tree's `PixelDeck` rename.

## Bugs found (documented as `test.fail` so the suite stays green and flips when fixed)

1. `rendererHelper.getImage`: `try { decodeURIComponent } finally { ... }` has no `catch`, so an SVG data URL with a lone `%` throws `URIError` instead of being passed through.
2. `rendererHelper.getImage`: raw (not percent-encoded) multi-line SVG data URLs are truncated at the first newline (`(.+)` does not match `\n`).
3. `rendererHelper.drawOverlay`: only `onload`; a missing/failed overlay asset leaves `renderImage` pending forever, which also holds the key `CanvasLock`.
4. `rendererHelper.resizeImage`: only `onload`; a corrupt image never settles, so choosing it in `InstanceEditor` silently does nothing.
5. Pasting a key onto a touch zone (cross-controller) rejects inside `Key.paste` -> `handlePaste` with no catch: unhandled promise rejection and no user feedback (pinned loosely in `keyContextMenuActions.pw.ts`).

Observations (not bugs): SVG data URLs with `;charset=utf-8` are not matched by the SVG normaliser (`\w` excludes `-`) and pass through unchanged, which still renders.

## Not covered and why

- Upper zoom clamp (3x) of the startup image resize handle: reaching it needs a drag longer than the preview panel allows in a 1440x900 window; the lower clamp (0.25) and growth beyond 1 are covered.
- Native colour picker popup (`input[type=color]` click): headless Chromium has no picker; the click wiring is asserted by spying `click()`, the resulting `change` is driven with `fill`.
- Real USB/HID, Tauri window APIs and plugin iframes' internal pages: need the Tauri shell (covered by Rust tests and manual hardware acceptance).
- Machine contention: the suite is ~400 tests against one Vite dev server; with all CPU cores busy (for example a concurrent `cargo build`) page loads can exceed the 45 s timeout. Use `--workers=5`.
