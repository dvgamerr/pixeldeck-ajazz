# Tech debt, qlty and Playwright plan

Status legend: [ ] todo, [~] in progress, [x] done

## Phase 0 - Baseline  [ ]
- [ ] 0.1 Create working branch
- [ ] 0.2 Run frontend verify (format:check, check, lint, bun test, build)
- [ ] 0.3 Run cargo fmt/clippy/test (src-tauri, ajazz-sdk)
- [ ] 0.4 `qlty init` + qlty.toml (exclude target, build, .svelte-kit, node_modules, examples)
- [ ] 0.5 Record qlty smells/metrics/check baseline in docs/QLTY_BASELINE.md

## Phase 1 - Tech debt audit  [ ]
- [ ] 1.1 Categorise findings (size, duplication, dead code, unwrap, CSS duplication, docs drift)
- [ ] 1.2 Order modules by hardware risk

## Phase 2 - Refactor (one commit per module)  [ ]
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
