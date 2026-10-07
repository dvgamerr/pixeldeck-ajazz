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

## Phase 2 - Refactor (one commit per module)  [~]
- [ ] 2.1 Frontend lib
- [ ] 2.2 Svelte components
- [ ] 2.3 Rust app (src-tauri/src)
- [ ] 2.4 ajazz-sdk (behaviour must not change, see CLAUDE.md)
- [ ] 2.5 plugins (starterpack, widgets)

## Phase 3 - Clear qlty  [ ]
- [ ] 3.1 smells -a -> 0
- [ ] 3.2 metrics -a within thresholds
- [ ] 3.3 check -a clean
- [ ] 3.4 CI step

## Phase 4 - Playwright  [ ]
- [ ] 4.1 Install @playwright/test, config, webServer
- [ ] 4.2 Tauri API mock
- [ ] 4.3 Specs: layout, Key menu, drag-drop, DeviceSelector race, ProfileManager, PluginManager, Settings, PropertyInspector
- [ ] 4.4 test:e2e script + CI

## Phase 5 - Wrap-up  [ ]
- [ ] 5.1 Full validation
- [ ] 5.2 Docs update, before/after summary

## Log
- Baseline: frontend verify green (13 bun tests, svelte-check 0/0). ajazz-sdk 18 tests pass. clippy clean.
- Baseline note: `cargo fmt --check` reports "Incorrect newline style" locally only because git autocrlf=true gives CRLF working copies (rustfmt.toml wants Unix). Not a code defect.
- qlty baseline smells: 17 files flagged (complexity: main, initialise_plugin, init_application_watcher, process_incoming_message, handle_input_state_change, renderImage, appear, init_webserver, update_profile_action_references ...; duplication: plugins pixel.rs x2, starterpack main.rs; many-params: curve_to, renderImage).
- qlty check baseline: shellcheck CRLF in scripts, osv-scanner advisories in Cargo.lock (glib, rustls, unic-*, proc-macro-error), prettier fmt on 3 json/test files, yamllint on ajazz-sdk qa.yaml, rustfmt plugin lacked edition (fixed via root rustfmt.toml).
- Phase 1 findings = qlty baseline above; risk order: frontend -> src-tauri/src -> sdk/plugins. Phase 2 running as 3 parallel workstreams (frontend / src-tauri app / sdk+plugins).
- [x] 2.4/2.5 ajazz-sdk (input_state.rs split) and plugins (shared/svg_text.rs, audio/system_monitor submodules) done; sdk 18 tests, starterpack 21, widgets 6 pass. Remaining: curve_to (trait-fixed 6 params) -> handle in Phase 3.
- [x] 2.1/2.2 frontend done and committed (rendererHelper options object, startupImage/profileFolders/plugins libs, KeyContextMenu etc., data-testid added). Known gap: PropertyInspectorView still mounted via {#if} in +page.svelte (CLAUDE.md says toggle visibility) -> fix after Playwright specs exist. Phase 4 agent started.
