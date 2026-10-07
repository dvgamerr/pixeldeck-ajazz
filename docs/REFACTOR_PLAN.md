# Tech debt, qlty and Playwright plan

Status legend: [ ] todo, [~] in progress, [x] done

## Phase 0 - Baseline  [x]
- [x] 0.1 Create working branch
- [x] 0.2 Run frontend verify (format:check, check, lint, bun test, build)
- [x] 0.3 Run cargo fmt/clippy/test (src-tauri, ajazz-sdk)
- [x] 0.4 `qlty init` + qlty.toml (exclude target, build, .svelte-kit, node_modules, examples)
- [x] 0.5 Record qlty smells/metrics/check baseline in docs/QLTY_BASELINE.md

## Phase 1 - Tech debt audit  [x]
- [x] 1.1 Categorise findings (size, duplication, dead code, unwrap, CSS duplication, docs drift)
- [x] 1.2 Order modules by hardware risk

## Phase 2 - Refactor (one commit per module)  [x]
- [x] 2.1 Frontend lib
- [x] 2.2 Svelte components
- [x] 2.3 Rust app (src-tauri/src)
- [x] 2.4 ajazz-sdk (behaviour must not change, see CLAUDE.md)
- [x] 2.5 plugins (starterpack, widgets)

## Phase 3 - Clear qlty  [x]
- [x] 3.1 smells -a -> 0
- [x] 3.2 metrics -a within thresholds
- [x] 3.3 check -a clean
- [x] 3.4 CI step

## Phase 4 - Playwright  [x]
- [x] 4.1 Install @playwright/test, config, webServer
- [x] 4.2 Tauri API mock
- [x] 4.3 Specs: layout, Key menu, drag-drop, DeviceSelector race, ProfileManager, PluginManager, Settings, PropertyInspector
- [x] 4.4 test:e2e script + CI

## Phase 5 - Wrap-up  [x]
- [x] 5.1 Full validation
- [x] 5.2 Docs update, before/after summary

## Log
- Baseline: frontend verify green (13 bun tests, svelte-check 0/0). ajazz-sdk 18 tests pass. clippy clean.
- Baseline note: `cargo fmt --check` reports "Incorrect newline style" locally only because git autocrlf=true gives CRLF working copies (rustfmt.toml wants Unix). Not a code defect.
- qlty baseline smells: 17 files flagged (complexity: main, initialise_plugin, init_application_watcher, process_incoming_message, handle_input_state_change, renderImage, appear, init_webserver, update_profile_action_references ...; duplication: plugins pixel.rs x2, starterpack main.rs; many-params: curve_to, renderImage).
- qlty check baseline: shellcheck CRLF in scripts, osv-scanner advisories in Cargo.lock (glib, rustls, unic-*, proc-macro-error), prettier fmt on 3 json/test files, yamllint on ajazz-sdk qa.yaml, rustfmt plugin lacked edition (fixed via root rustfmt.toml).
- Phase 1 findings = qlty baseline above; risk order: frontend -> src-tauri/src -> sdk/plugins. Phase 2 running as 3 parallel workstreams (frontend / src-tauri app / sdk+plugins).
- [x] 2.4/2.5 ajazz-sdk (input_state.rs split) and plugins (shared/svg_text.rs, audio/system_monitor submodules) done; sdk 18 tests, starterpack 21, widgets 6 pass. Remaining: curve_to (trait-fixed 6 params) -> handle in Phase 3.
- [x] 2.1/2.2 frontend done and committed (rendererHelper options object, startupImage/profileFolders/plugins libs, KeyContextMenu etc., data-testid added). Known gap: PropertyInspectorView still mounted via {#if} in +page.svelte (CLAUDE.md says toggle visibility) -> fix after Playwright specs exist. Phase 4 agent started.
- [x] 2.3 src-tauri/src refactor done (main/ajazz/watcher/plugins/store split), clippy clean, 19 tests. plugins/shared moved to repo-root shared/ because src-tauri/build.rs treats every dir under plugins/ as a plugin.
- Phase 3 result: `qlty smells -a` = 0 findings. Config: smells.function_parameters threshold 7 (ttf_parser trait curve_to has 6), e2e/fixtures/tauriMock.ts excluded (in-page self-contained mock script), osv-scanner.toml next to each Cargo.lock ignoring Tauri-Linux transitive advisories, rustls bumped.
- Phase 3 not fixed (infra, pre-existing): zizmor unpinned action/image refs and template-injection in release.yml, Dockerfile hadolint/radarlint findings, yamllint CRLF on working copy, qlty rustfmt "fmt" notes caused by CRLF working copies (cargo fmt --check passes on all 4 manifests), shellcheck CRLF (scripts/*.sh excluded; index is LF).
- Phase 4 result: 96 Playwright tests (e2e/*.pw.ts) pass, twice-repeat stable. Run: `bun run test:e2e`. Fixed during this: context menu clamp width 128->144, property inspector now stays mounted (hidden class).
- Phase 5: verify (format, check, lint, bun test 13, build), cargo clippy/test on 4 manifests, Playwright all green. Hardware acceptance (Registered Ajazz AKP05E_552A as sd-<serial>) NOT run - needs a physical device.

## Phase 6 - Unit tests for every function (Playwright)  [~]
Scope: all functions under src/lib (TS) and all Svelte component logic. Rust functions cannot be driven by Playwright; they stay on `cargo test` (separate decision).
- [ ] 6.1 `playwright.unit.config.ts` + `unit/` (node, no browser) and `test:unit` script
- [ ] 6.2 Pure/store functions in src/lib (node project)
- [ ] 6.3 DOM/canvas-dependent lib functions (rendererHelper, startupImage, portal, ...) run in browser context
- [ ] 6.4 Component behaviour gaps not covered by e2e (ParentActionView, NoDevicesDetected, InstanceEditor formatting, StartupImage editor, ReleaseAssetChooser, PluginDetails)
- [ ] 6.5 Coverage report: list every exported function and its test; fill gaps
- [ ] 6.6 CI job + docs
Note: working tree had uncommitted user edits (product rename in package.json, tauri.conf, etc.) when Phase 6 started; they are left untouched.
