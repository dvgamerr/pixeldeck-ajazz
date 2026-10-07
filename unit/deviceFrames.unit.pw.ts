import { expect, test } from "@playwright/test";

import { type Context } from "$lib/Context";
import { installTauriMock, uninstallTauriMock } from "./helpers/tauri";
import { freshImport, sleep } from "./helpers/fresh";

test.afterEach(() => uninstallTauriMock());

const ctx = (position: number, device = "dev"): Context => ({ device, profile: "P", controller: "Keypad", position });

test.describe("deviceFrames", () => {
	test("queueDeviceFrame sends a batched update_images invoke after the live window", async () => {
		const tauri = installTauriMock();
		const { queueDeviceFrame } = await freshImport("deviceFrames.ts");
		queueDeviceFrame(ctx(0), "data:a");
		queueDeviceFrame(ctx(1), null);
		await sleep(60);
		expect(tauri.calls).toHaveLength(1);
		expect(tauri.calls[0].cmd).toBe("update_images");
		expect(tauri.calls[0].args.frames).toEqual([
			{ context: ctx(0), image: "data:a" },
			{ context: ctx(1), image: null },
		]);
	});

	test("the queued context is copied so later mutation does not leak", async () => {
		const tauri = installTauriMock();
		const { queueDeviceFrame } = await freshImport("deviceFrames.ts");
		const context = ctx(2);
		queueDeviceFrame(context, "x");
		context.position = 99;
		await sleep(60);
		expect(tauri.calls[0].args.frames[0].context.position).toBe(2);
	});

	test("beginInitialDeviceRender holds frames until the expected count is reached", async () => {
		const tauri = installTauriMock();
		const { beginInitialDeviceRender, queueDeviceFrame } = await freshImport("deviceFrames.ts");
		beginInitialDeviceRender("dev", "P", 2);
		queueDeviceFrame(ctx(0), "a");
		await sleep(60);
		expect(tauri.calls).toHaveLength(0);
		queueDeviceFrame(ctx(1), "b");
		await sleep(10);
		expect(tauri.calls).toHaveLength(1);
		expect(tauri.calls[0].args.frames).toHaveLength(2);
	});

	test("cancelDeviceFrames drops pending frames for that device", async () => {
		const tauri = installTauriMock();
		const { beginInitialDeviceRender, queueDeviceFrame, cancelDeviceFrames } = await freshImport("deviceFrames.ts");
		beginInitialDeviceRender("dev", "P", 3);
		queueDeviceFrame(ctx(0), "a");
		cancelDeviceFrames("dev");
		await sleep(60);
		expect(tauri.calls).toHaveLength(0);
	});

	test("a rejecting invoke is swallowed with a console warning", async () => {
		installTauriMock(() => {
			throw new Error("backend down");
		});
		const warnings: unknown[][] = [];
		const original = console.warn;
		console.warn = (...args: unknown[]) => void warnings.push(args);
		try {
			const { queueDeviceFrame } = await freshImport("deviceFrames.ts");
			queueDeviceFrame(ctx(0), "a");
			await sleep(60);
		} finally {
			console.warn = original;
		}
		expect(warnings.length).toBeGreaterThan(0);
	});
});
