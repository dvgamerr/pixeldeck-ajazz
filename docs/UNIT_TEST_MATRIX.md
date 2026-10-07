# Unit test matrix for `src/lib`

Run with `bun run test:unit` (Playwright Test runner, no browser, no web server; config: `playwright.unit.config.ts`, files: `unit/**/*.unit.pw.ts`).
Tauri `invoke` is stubbed through `globalThis.__TAURI_INTERNALS__` (`unit/helpers/tauri.ts`); stateful modules are re-imported per test (`unit/helpers/fresh.ts`).

Status: `covered` = unit test exists, `browser-context: pending` = needs a real DOM/canvas and is left to the browser-context suite.

## Pure / store modules

| Module | Export | Status | Test file / describe |
| --- | --- | --- | --- |
| Action.ts | `Action` (type) | covered | `types.unit.pw.ts` (shape via `satisfies`) |
| ActionInstance.ts | `ActionInstance` (type) | covered | `types.unit.pw.ts` |
| ActionState.ts | `ActionState` (type) | covered | `types.unit.pw.ts` |
| DeviceInfo.ts | `DeviceInfo` (type) | covered | `types.unit.pw.ts` |
| Profile.ts | `Profile` (type) | covered | `types.unit.pw.ts` |
| Context.ts | `Context` (type) | covered | `Context.unit.pw.ts` |
| Context.ts | `contextsEqual` | covered | `Context.unit.pw.ts` > contextsEqual |
| DeviceFrameCoordinator.ts | `DeviceFrame` (type) | covered | `DeviceFrameCoordinator.unit.pw.ts` |
| DeviceFrameCoordinator.ts | `DeviceFrameCoordinator` (`beginInitialRender`, `queue`, `cancel`, `flushPending`, constructor defaults) | covered | `DeviceFrameCoordinator.unit.pw.ts` > initial render / live frames / flushPending / cancel / defaults |
| applicationProfiles.ts | `ApplicationProfiles` (type) | covered | `types.unit.pw.ts` |
| applicationProfiles.ts | `DEFAULT_APPLICATION` | covered | `applicationProfiles.unit.pw.ts` |
| applicationProfiles.ts | `cleanApplicationProfiles` | covered | `applicationProfiles.unit.pw.ts` > cleanApplicationProfiles |
| applicationProfiles.ts | `sortApplicationEntries` | covered | `applicationProfiles.unit.pw.ts` > sortApplicationEntries |
| applicationProfiles.ts | `isUnmapped` | covered | `applicationProfiles.unit.pw.ts` > isUnmapped |
| deviceFrames.ts | `beginInitialDeviceRender` | covered | `deviceFrames.unit.pw.ts` |
| deviceFrames.ts | `queueDeviceFrame` | covered | `deviceFrames.unit.pw.ts` |
| deviceFrames.ts | `cancelDeviceFrames` | covered | `deviceFrames.unit.pw.ts` |
| imageFormat.ts | `isGifImageSource` | covered | `imageFormat.unit.pw.ts` |
| plugins.ts | `GitHubPlugin`, `ReleaseAsset` (types) | covered | `types.unit.pw.ts` |
| plugins.ts | `getFetch` | covered | `plugins.unit.pw.ts` > getFetch |
| plugins.ts | `isInstallableAsset` | covered | `plugins.unit.pw.ts` |
| plugins.ts | `releasesEndpoint` | covered | `plugins.unit.pw.ts` |
| plugins.ts | `sortInstalledPlugins` | covered | `plugins.unit.pw.ts` |
| plugins.ts | `matchesQuery` | covered | `plugins.unit.pw.ts` |
| ports.ts | `initPortBase` | covered | `ports.unit.pw.ts` |
| ports.ts | `getWebSocketPort` | covered | `ports.unit.pw.ts` |
| ports.ts | `getWebserverUrl` | covered | `ports.unit.pw.ts` |
| profileFolders.ts | `ProfileFolders` (type) | covered | `types.unit.pw.ts` |
| profileFolders.ts | `makeFolders` | covered | `profileFolders.unit.pw.ts` |
| profileFolders.ts | `addToFolders` | covered | `profileFolders.unit.pw.ts` |
| profileRendering.ts | `pausedProfileRenderingDevices` (store) | covered | `profileRendering.unit.pw.ts` |
| profileRendering.ts | `pauseProfileRendering` | covered | `profileRendering.unit.pw.ts` |
| profileRendering.ts | `resumeProfileRendering` | covered | `profileRendering.unit.pw.ts` |
| profileRendering.ts | `getPausedProfileRenderingDevices` | covered | `profileRendering.unit.pw.ts` |
| propertyInspector.ts | `inspectedInstance` | covered | `propertyInspector.unit.pw.ts` (document stubbed) |
| propertyInspector.ts | `inspectedParentAction` | covered | `propertyInspector.unit.pw.ts` |
| propertyInspector.ts | `openContextMenu` (incl. document click closes it) | covered | `propertyInspector.unit.pw.ts` |
| propertyInspector.ts | `copiedContext` | covered | `propertyInspector.unit.pw.ts` |
| settings.ts | `Settings` (type) | covered | `types.unit.pw.ts` |
| settings.ts | `settings` (store) | covered | `settings.unit.pw.ts` |
| settings.ts | `localisations` (store) | covered | `settings.unit.pw.ts` |
| singletons.ts | `PRODUCT_NAME` | covered | `singletons.unit.pw.ts` |
| singletons.ts | `actionList`, `deviceSelector`, `profileManager` | covered | `singletons.unit.pw.ts` |
| startup.ts | `StartupTask` (type) | covered | `types.unit.pw.ts` |
| startup.ts | `showStartupTask` | covered | `startup.unit.pw.ts` (document/window stubbed) |
| startup.ts | `completeStartupTask` | covered | `startup.unit.pw.ts` |
| shims.ts | side effect: `globalThis.open` override | covered | `shims.unit.pw.ts` |
| rendererHelper.ts | `getImage` | covered | `rendererHelper.unit.pw.ts` > getImage |
| rendererHelper.ts | `CanvasLock` | covered | `rendererHelper.unit.pw.ts` > CanvasLock |
| rendererHelper.ts | `RenderImageOptions` (type) | covered (type only) | n/a |

## startupImage.ts

| Export | Status | Test file / describe |
| --- | --- | --- |
| `MAX_LAYERS`, `PREVIEW_PADDING`, `PREVIEW_PANEL_*_PADDING` | covered | `startupImage.unit.pw.ts` > constants |
| `AKP05_MASK` | covered | `startupImage.unit.pw.ts` > constants |
| `RESIZE_HANDLES` | covered | `startupImage.unit.pw.ts` > constants |
| `getFittedImageSize` | covered | `startupImage.unit.pw.ts` |
| `getTransformBounds` | covered | `startupImage.unit.pw.ts` |
| `rotateVector` | covered | `startupImage.unit.pw.ts` |
| `makeLayerId` | covered | `startupImage.unit.pw.ts` |
| `partitionFiles` | covered | `startupImage.unit.pw.ts` |
| `decodeImage` | covered with stubbed `Image`; real decoding is browser-context: pending | `startupImage.unit.pw.ts` |
| `drawComposedImage` | covered with a fake canvas/2d context; pixel output is browser-context: pending | `startupImage.unit.pw.ts` |
| `readFile` | raster (png/bmp/jpg) and error paths covered with stubbed `FileReader`; SVG sanitising (DOMPurify + DOMParser) is browser-context: pending | `startupImage.unit.pw.ts` |
| `PersistedLayer`, `ImageLayer`, `StartupImageProject`, `Size` (types) | covered (type only) | n/a |

## Browser-context: pending

| Module | Export | Reason |
| --- | --- | --- |
| rendererHelper.ts | `renderImage` | canvas 2D drawing, `document.fonts`, `Image` loading |
| rendererHelper.ts | `resizeImage` | canvas + `toDataURL`, `Image` loading (the GIF early return is trivially pure but untested here) |
| rendererHelper.ts | internal `loadImage` cache (not exported) | reached only through `renderImage` |
| portal.ts | `portalToBody` | `document.body` DOM node moves |
| portal.ts | `portalToPreviewDock` | `document.querySelector(".device-workspace")` |
| startupImage.ts | SVG branch of `readFile` (`sanitizeSvgToDataUrl`, not exported) | DOMPurify needs a real `window`; `DOMParser` |
| startupImage.ts | real image decode / real canvas pixels (`decodeImage`, `drawComposedImage`) | need a real browser to verify output |

## Known findings

- `rendererHelper.getImage`: the `try { decodeURIComponent } finally {...}` has no `catch`, so a malformed percent sequence in a raw `data:image/svg+xml,` URL throws `URIError` out of the function. Pinned by the test "malformed percent-encoding in an SVG data URL currently throws URIError"; update that test when fixed.
- `plugins.releasesEndpoint` yields `//releases` for a repository URL with a trailing slash (pinned by a test).
