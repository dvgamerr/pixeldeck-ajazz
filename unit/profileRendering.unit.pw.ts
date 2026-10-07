import { expect, test } from "@playwright/test";
import { get } from "svelte/store";

import { installTauriMock, uninstallTauriMock } from "./helpers/tauri";
import { freshImport, sleep } from "./helpers/fresh";

test.afterEach(() => uninstallTauriMock());

test.describe("profileRendering", () => {
	test("starts with no paused devices", async () => {
		const mod = await freshImport("profileRendering.ts");
		expect(mod.getPausedProfileRenderingDevices()).toEqual([]);
		expect([...get(mod.pausedProfileRenderingDevices)]).toEqual([]);
	});

	test("pauseProfileRendering returns true the first time and false when already paused", async () => {
		const mod = await freshImport("profileRendering.ts");
		expect(mod.pauseProfileRendering("d1")).toBe(true);
		expect(mod.pauseProfileRendering("d1")).toBe(false);
		expect(mod.getPausedProfileRenderingDevices()).toEqual(["d1"]);
	});

	test("the store emits a new Set each change and never mutates an emitted one", async () => {
		const mod = await freshImport("profileRendering.ts");
		const seen: ReadonlySet<string>[] = [];
		const unsubscribe = mod.pausedProfileRenderingDevices.subscribe((value: ReadonlySet<string>) => seen.push(value));
		mod.pauseProfileRendering("d1");
		mod.pauseProfileRendering("d2");
		mod.resumeProfileRendering("d1");
		unsubscribe();
		expect(seen.map((s) => [...s])).toEqual([[], ["d1"], ["d1", "d2"], ["d2"]]);
		expect(new Set(seen).size).toBe(4);
	});

	test("resumeProfileRendering on an unpaused device is a no-op and does not emit", async () => {
		const mod = await freshImport("profileRendering.ts");
		let emissions = 0;
		const unsubscribe = mod.pausedProfileRenderingDevices.subscribe(() => emissions++);
		mod.resumeProfileRendering("ghost");
		unsubscribe();
		expect(emissions).toBe(1); // only the initial subscribe call
	});

	test("pausing cancels pending device frames", async () => {
		const tauri = installTauriMock();
		const frames = await freshImport("deviceFrames.ts");
		const mod = await freshImport("profileRendering.ts");
		// profileRendering binds to the shared (non-fresh) deviceFrames instance.
		const shared = await import("../src/lib/deviceFrames.ts");
		shared.beginInitialDeviceRender("pause-dev", "P", 3);
		shared.queueDeviceFrame({ device: "pause-dev", profile: "P", controller: "Keypad", position: 0 }, "a");
		mod.pauseProfileRendering("pause-dev");
		await sleep(60);
		expect(tauri.calls).toHaveLength(0);
		expect(frames).toBeTruthy();
	});

	test("paused devices keep insertion order and resume removes just one", async () => {
		const mod = await freshImport("profileRendering.ts");
		mod.pauseProfileRendering("a");
		mod.pauseProfileRendering("b");
		mod.pauseProfileRendering("c");
		mod.resumeProfileRendering("b");
		expect(mod.getPausedProfileRenderingDevices()).toEqual(["a", "c"]);
	});

	test("getPausedProfileRenderingDevices returns a copy", async () => {
		const mod = await freshImport("profileRendering.ts");
		mod.pauseProfileRendering("a");
		const list = mod.getPausedProfileRenderingDevices();
		list.push("zzz");
		expect(mod.getPausedProfileRenderingDevices()).toEqual(["a"]);
	});
});
