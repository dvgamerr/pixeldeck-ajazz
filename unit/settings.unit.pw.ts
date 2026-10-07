import { expect, test } from "@playwright/test";
import { get } from "svelte/store";

import { installTauriMock, uninstallTauriMock } from "./helpers/tauri";
import { flushMicrotasks, freshImport } from "./helpers/fresh";

test.afterEach(() => uninstallTauriMock());

const sample = {
	version: "1",
	language: "th",
	brightness: 40,
	darktheme: true,
	background: false,
	autolaunch: false,
	updatecheck: true,
	statistics: false,
	separatewine: false,
	developer: false,
	disabledevices: false,
};

test.describe("settings store", () => {
	test("loads the backend value from get_settings", async () => {
		const tauri = installTauriMock((cmd) => (cmd == "get_settings" ? sample : cmd == "get_localisations" ? { p: {} } : undefined));
		const mod = await freshImport("settings.ts");
		await flushMicrotasks();
		expect(get(mod.settings)).toEqual(sample);
		expect(tauri.calls[0].cmd).toBe("get_settings");
	});

	test("a loaded value is written back with set_settings and localisations are fetched for its language", async () => {
		const tauri = installTauriMock((cmd) => (cmd == "get_settings" ? sample : cmd == "get_localisations" ? { plugin: { Hello: "สวัสดี" } } : undefined));
		const mod = await freshImport("settings.ts");
		await flushMicrotasks();
		const cmds = tauri.calls.map((c) => c.cmd);
		expect(cmds).toContain("set_settings");
		expect(cmds).toContain("get_localisations");
		expect(tauri.calls.find((c) => c.cmd == "set_settings")?.args).toEqual({ settings: sample });
		expect(tauri.calls.find((c) => c.cmd == "get_localisations")?.args).toEqual({ locale: "th" });
		expect(get(mod.localisations)).toEqual({ plugin: { Hello: "สวัสดี" } });
	});

	test("updating the store persists again and reloads localisations for the new language", async () => {
		const tauri = installTauriMock((cmd, args) => (cmd == "get_settings" ? sample : cmd == "get_localisations" ? { lang: args.locale } : undefined));
		const mod = await freshImport("settings.ts");
		await flushMicrotasks();
		mod.settings.set({ ...sample, language: "en", brightness: 90 });
		await flushMicrotasks();
		const sets = tauri.calls.filter((c) => c.cmd == "set_settings");
		expect(sets).toHaveLength(2);
		expect(sets[1].args.settings.brightness).toBe(90);
		expect(get(mod.localisations)).toEqual({ lang: "en" });
	});

	test("setting null does not call the backend", async () => {
		const tauri = installTauriMock((cmd) => (cmd == "get_settings" ? null : undefined));
		const mod = await freshImport("settings.ts");
		await flushMicrotasks();
		const before = tauri.calls.length;
		mod.settings.set(null);
		await flushMicrotasks();
		expect(tauri.calls).toHaveLength(before);
		expect(tauri.calls.map((c) => c.cmd)).not.toContain("set_settings");
		expect(get(mod.localisations)).toBeNull();
	});

	test("localisations starts as null", async () => {
		installTauriMock(() => null);
		const mod = await freshImport("settings.ts");
		expect(get(mod.localisations)).toBeNull();
	});
});
