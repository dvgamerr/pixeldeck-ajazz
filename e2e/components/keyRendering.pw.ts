import type { Page } from "@playwright/test";

import { DIAL_ACTION, KEY_ACTION, makeAction, openApp, test, expect, type TauriMock } from "../fixtures/tauriMock";

// makeAction() states have an empty image, which renders as "icon unavailable"; give these a real (mocked) icon.
const withIcon = <T extends { states: any[] }>(action: T): T => ({ ...action, states: action.states.map((state) => ({ ...state, image: "icon.png" })) });
const PAINTED_KEY = withIcon(KEY_ACTION);
const PAINTED_DIAL = withIcon(DIAL_ACTION);

const KEY_CONTEXT = { device: "sd-TEST", profile: "Default", controller: "Keypad", position: 2 };
const KEY_INSTANCE = "sd-TEST.Default.Keypad.2.0";
const GIF = "data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==";

const canvasOf = (page: Page, testId: string) => page.getByTestId(testId).getByTestId("key-canvas");
const dataUrl = (page: Page, testId: string) => canvasOf(page, testId).evaluate((node) => (node as HTMLCanvasElement).toDataURL());
const alphaAt = (page: Page, testId: string, x: number, y: number) =>
	canvasOf(page, testId).evaluate((node, point) => (node as HTMLCanvasElement).getContext("2d")!.getImageData(point.x, point.y, 1, 1).data[3]!, { x, y });

/** Frames sent to the device for one slot, newest last. */
const framesFor = async (tauri: TauriMock, controller: string, position: number) => {
	const frames: { context: any; image: string | null }[] = [];
	for (const call of await tauri.calls("update_images")) {
		for (const frame of call.args.frames) if (frame.context.controller == controller && frame.context.position == position) frames.push(frame);
	}
	return frames;
};

test.describe("Key rendering", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 2, action: PAINTED_KEY }] } });

	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("canvas geometry follows the AKP05E_552A formats", async ({ page }) => {
		const sizes = await page.evaluate(() => ({
			key: [(document.querySelector('[data-testid="device-key-0"] canvas') as HTMLCanvasElement).width, (document.querySelector('[data-testid="device-key-0"] canvas') as HTMLCanvasElement).height],
			touch: [
				(document.querySelector('[data-testid="device-touch-0"] canvas') as HTMLCanvasElement).width,
				(document.querySelector('[data-testid="device-touch-0"] canvas') as HTMLCanvasElement).height,
			],
		}));
		expect(sizes.key).toEqual([126, 126]);
		expect(sizes.touch).toEqual([176, 112]);
		const touchBox = (await canvasOf(page, "device-touch-0").boundingBox())!;
		expect([Math.round(touchBox.width), Math.round(touchBox.height)]).toEqual([160, 100]);
	});

	test("an occupied slot is painted and an empty one stays transparent", async ({ page }) => {
		await expect.poll(() => alphaAt(page, "device-key-2", 63, 63)).toBe(255);
		expect(await alphaAt(page, "device-key-3", 63, 63)).toBe(0);
	});

	test("occupied and empty slots are flagged in data-occupied, controller and position", async ({ page }) => {
		const key = page.getByTestId("device-key-2");
		await expect(key).toHaveAttribute("data-occupied", "true");
		await expect(key).toHaveAttribute("data-controller", "Keypad");
		await expect(key).toHaveAttribute("data-position", "2");
		await expect(page.getByTestId("device-key-3")).toHaveAttribute("data-occupied", "false");
		await expect(page.getByTestId("device-touch-1")).toHaveAttribute("data-controller", "Encoder");
	});

	test("the selected key gets an outline and deselecting removes it", async ({ page }) => {
		const canvas = canvasOf(page, "device-key-2");
		await expect(canvas).not.toHaveClass(/outline-solid/);
		await canvas.click();
		await expect(canvas).toHaveClass(/outline-solid/);
		await page.getByTestId("device-view").click({ position: { x: 5, y: 5 } });
		await expect(canvas).not.toHaveClass(/outline-solid/);
	});

	test("clicking an empty slot selects nothing", async ({ page, tauri }) => {
		await canvasOf(page, "device-key-3").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
		expect((await tauri.calls("switch_property_inspector")).filter((call) => call.args.new)).toEqual([]);
	});

	test("ctrl+click does not select (it is reserved for the browser)", async ({ page }) => {
		await canvasOf(page, "device-key-2").click({ modifiers: ["Control"] });
		await expect(canvasOf(page, "device-key-2")).not.toHaveClass(/outline-solid/);
	});

	test("the initial render is sent to the device as a single batch of 14 frames", async ({ tauri }) => {
		await expect.poll(async () => (await tauri.calls("update_images")).length).toBeGreaterThan(0);
		const [first] = await tauri.calls("update_images");
		expect(first!.args.frames).toHaveLength(14);
		const frames = first!.args.frames as { context: any; image: string | null }[];
		const key = frames.find((frame) => frame.context.controller == "Keypad" && frame.context.position == 2)!;
		expect(key.context).toEqual(KEY_CONTEXT);
		expect(key.image).toMatch(/^data:image\/jpeg;base64,/);
		const empty = frames.find((frame) => frame.context.controller == "Keypad" && frame.context.position == 3)!;
		expect(empty.image).toBeNull();
	});

	test("pressing the physical key shrinks the picture and releasing restores it", async ({ page, tauri }) => {
		await expect.poll(() => alphaAt(page, "device-key-2", 3, 3)).toBe(255);
		await tauri.emit("key_moved", { context: KEY_CONTEXT, pressed: true });
		await expect.poll(() => alphaAt(page, "device-key-2", 3, 3)).toBe(0);
		expect(await alphaAt(page, "device-key-2", 63, 63)).toBe(255);
		await tauri.emit("key_moved", { context: KEY_CONTEXT, pressed: false });
		await expect.poll(() => alphaAt(page, "device-key-2", 3, 3)).toBe(255);
	});

	test("key_moved for a different slot does not affect this key", async ({ page, tauri }) => {
		await expect.poll(() => alphaAt(page, "device-key-2", 3, 3)).toBe(255);
		await tauri.emit("key_moved", { context: { ...KEY_CONTEXT, position: 5 }, pressed: true });
		await tauri.emit("key_moved", { context: { ...KEY_CONTEXT, controller: "Encoder" }, pressed: true });
		await page.waitForTimeout(200);
		expect(await alphaAt(page, "device-key-2", 3, 3)).toBe(255);
	});

	test("a pressed key is also pushed to the device as a smaller frame", async ({ tauri }) => {
		await expect.poll(async () => (await framesFor(tauri, "Keypad", 2)).length).toBeGreaterThan(0);
		const before = (await framesFor(tauri, "Keypad", 2)).length;
		await tauri.emit("key_moved", { context: KEY_CONTEXT, pressed: true });
		await expect.poll(async () => (await framesFor(tauri, "Keypad", 2)).length).toBeGreaterThan(before);
	});

	test("show_ok draws the OK overlay for 1.5 seconds", async ({ page, tauri }) => {
		await expect.poll(() => alphaAt(page, "device-key-2", 63, 63)).toBe(255);
		const base = await dataUrl(page, "device-key-2");
		await tauri.emit("show_ok", KEY_INSTANCE);
		await expect.poll(() => dataUrl(page, "device-key-2")).not.toBe(base);
		await expect.poll(() => dataUrl(page, "device-key-2"), { timeout: 5000 }).toBe(base);
	});

	test("show_alert draws a different overlay from show_ok, and replaces a running one", async ({ page, tauri }) => {
		await expect.poll(() => alphaAt(page, "device-key-2", 63, 63)).toBe(255);
		const base = await dataUrl(page, "device-key-2");
		await tauri.emit("show_ok", KEY_INSTANCE);
		await expect.poll(() => dataUrl(page, "device-key-2")).not.toBe(base);
		const ok = await dataUrl(page, "device-key-2");
		await tauri.emit("show_alert", KEY_INSTANCE);
		await expect.poll(() => dataUrl(page, "device-key-2")).not.toBe(ok);
		expect(await dataUrl(page, "device-key-2")).not.toBe(base);
	});

	test("show_ok and show_alert for another instance are ignored", async ({ page, tauri }) => {
		await expect.poll(() => alphaAt(page, "device-key-2", 63, 63)).toBe(255);
		const base = await dataUrl(page, "device-key-2");
		await tauri.emit("show_ok", "sd-TEST.Default.Keypad.7.0");
		await tauri.emit("show_alert", "sd-OTHER.Default.Keypad.2.0");
		await page.waitForTimeout(300);
		expect(await dataUrl(page, "device-key-2")).toBe(base);
	});

	test("update_state with new contents redraws the key, other contexts are ignored", async ({ page, tauri }) => {
		await expect.poll(() => alphaAt(page, "device-key-2", 63, 63)).toBe(255);
		const base = await dataUrl(page, "device-key-2");
		const instance = await tauri.mockState(`(s) => JSON.parse(JSON.stringify(s.profiles["sd-TEST/Default"].keys[2]))`);
		instance.states[0].show = true;
		instance.states[0].text = "Hello";

		await tauri.emit("update_state", { context: "sd-TEST.Default.Keypad.9.0", contents: instance });
		await page.waitForTimeout(250);
		expect(await dataUrl(page, "device-key-2")).toBe(base);

		await tauri.emit("update_state", { context: KEY_INSTANCE, contents: instance });
		await expect.poll(() => dataUrl(page, "device-key-2")).not.toBe(base);
	});

	test("update_state with null contents empties the slot", async ({ page, tauri }) => {
		await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "true");
		await tauri.emit("update_state", { context: KEY_INSTANCE, contents: null });
		await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "false");
		await expect.poll(() => alphaAt(page, "device-key-2", 63, 63)).toBe(0);
		await expect.poll(async () => (await framesFor(tauri, "Keypad", 2)).at(-1)?.image).toBeNull();
	});

	test("every key registers its four Tauri event listeners", async ({ tauri }) => {
		for (const event of ["update_state", "key_moved", "show_ok", "show_alert"]) {
			// 10 keys + 4 touch zones.
			expect(await tauri.listenerCount(event), event).toBe(14);
		}
	});
});

test.describe("Key rendering: images", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 2, action: PAINTED_KEY }] } });

	test("a static image stops producing frames once rendered (no animation loop)", async ({ page, tauri }) => {
		await openApp(page);
		await expect.poll(async () => (await tauri.calls("update_images")).length).toBeGreaterThan(0);
		await page.waitForTimeout(500);
		const settled = (await framesFor(tauri, "Keypad", 2)).length;
		await page.waitForTimeout(800);
		expect((await framesFor(tauri, "Keypad", 2)).length).toBe(settled);
	});

	test.describe("animated GIF", () => {
		test.use({ options: { seed: [{ controller: "Keypad", position: 2, action: { ...KEY_ACTION, states: [{ ...KEY_ACTION.states[0], image: GIF }] } }] } });

		test("keeps re-rendering and streaming frames to the device", async ({ page, tauri }) => {
			await openApp(page);
			await expect.poll(async () => (await framesFor(tauri, "Keypad", 2)).length, { timeout: 8000 }).toBeGreaterThanOrEqual(4);
			const frames = await framesFor(tauri, "Keypad", 2);
			for (const frame of frames) expect(frame.image).toMatch(/^data:image\/jpeg;base64,/);
		});

		test("stops streaming once the key is removed", async ({ page, tauri }) => {
			await openApp(page);
			await expect.poll(async () => (await framesFor(tauri, "Keypad", 2)).length, { timeout: 8000 }).toBeGreaterThanOrEqual(2);
			await canvasOf(page, "device-key-2").click({ button: "right" });
			await page.getByTestId("context-menu-delete").click();
			await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "false");
			await expect.poll(async () => (await framesFor(tauri, "Keypad", 2)).at(-1)?.image).toBeNull();
			const settled = (await framesFor(tauri, "Keypad", 2)).length;
			await page.waitForTimeout(700);
			expect((await framesFor(tauri, "Keypad", 2)).length).toBe(settled);
		});
	});
});

test.describe("Key rendering: loading animation", () => {
	test("shows while the icon is still downloading, then disappears", async ({ page }) => {
		let release!: () => void;
		const gate = new Promise<void>((resolve) => (release = resolve));
		await page.route("http://127.0.0.1:*/**", async (route) => {
			await gate;
			await route.fallback();
		});
		await openApp(page);
		// Nothing configured yet in this scenario: seed via a drop so the icon request is in flight.
		const transfer = await page.evaluateHandle((action) => {
			const data = new DataTransfer();
			data.setData("application/x-opendeck-action", JSON.stringify(action));
			return data;
		}, PAINTED_KEY);
		const canvas = canvasOf(page, "device-key-4");
		await canvas.dispatchEvent("dragover", { dataTransfer: transfer });
		await canvas.dispatchEvent("drop", { dataTransfer: transfer });
		await expect(page.getByTestId("device-key-4")).toHaveAttribute("data-occupied", "true");
		await expect(page.getByTestId("device-key-4").getByTestId("key-loading")).toBeVisible();
		expect(await alphaAt(page, "device-key-4", 63, 63)).toBe(0);
		release();
		await expect(page.getByTestId("device-key-4").getByTestId("key-loading")).toHaveCount(0);
		await expect.poll(() => alphaAt(page, "device-key-4", 63, 63)).toBe(255);
	});

	test.describe("action without any icon", () => {
		const NO_ICON = { ...makeAction("test.plugin.noicon", "No Icon", ["Keypad"]), icon: "" };
		test.use({ options: { seed: [{ controller: "Keypad", position: 1, action: NO_ICON }] } });

		test("keeps the loading animation instead of drawing the alert icon", async ({ page }) => {
			await openApp(page);
			await expect(page.getByTestId("device-key-1").getByTestId("key-loading")).toBeVisible();
			await expect(page.getByTestId("device-key-1").getByRole("status", { name: "Loading" })).toBeVisible();
			expect(await alphaAt(page, "device-key-1", 63, 63)).toBe(0);
		});
	});

	test.describe("healthy keys", () => {
		test.use({ options: { seed: [{ controller: "Keypad", position: 2, action: PAINTED_KEY }] } });

		test("do not show the loading animation once rendered", async ({ page }) => {
			await openApp(page);
			await expect.poll(() => alphaAt(page, "device-key-2", 63, 63)).toBe(255);
			await expect(page.getByTestId("key-loading")).toHaveCount(0);
		});
	});
});

test.describe("Key rendering: encoder zones", () => {
	test.use({ options: { seed: [{ controller: "Encoder", position: 1, action: PAINTED_DIAL }] } });

	test("touch zones render their action and flag the Encoder controller", async ({ page, tauri }) => {
		await openApp(page);
		const zone = page.getByTestId("device-touch-1");
		await expect(zone).toHaveAttribute("data-occupied", "true");
		await expect.poll(() => alphaAt(page, "device-touch-1", 88, 56)).toBe(255);
		await expect.poll(async () => (await framesFor(tauri, "Encoder", 1)).at(-1)?.image).toMatch(/^data:image\/jpeg/);
	});

	test("key_moved with the Encoder context presses only that zone", async ({ page, tauri }) => {
		await openApp(page);
		await expect.poll(() => alphaAt(page, "device-touch-1", 3, 3)).toBe(255);
		await tauri.emit("key_moved", { context: { device: "sd-TEST", profile: "Default", controller: "Encoder", position: 1 }, pressed: true });
		await expect.poll(() => alphaAt(page, "device-touch-1", 3, 3)).toBe(0);
	});
});

test.describe("Key rendering: large sd- devices", () => {
	test.use({ options: { devices: { "sd-BIG": { id: "sd-BIG", name: "Big Deck", rows: 4, columns: 8, encoders: 2, type: 0 } } } });

	test("a 4x8 sd- device renders 192px key canvases scaled to the preview size", async ({ page }) => {
		await openApp(page);
		const info = await canvasOf(page, "device-key-0").evaluate((node) => ({ width: (node as HTMLCanvasElement).width, height: (node as HTMLCanvasElement).height, style: node.getAttribute("style") }));
		expect(info.width).toBe(192);
		expect(info.height).toBe(192);
		expect(info.style).toContain("width: 192px");
		expect(info.style).toMatch(/scale\(0\.58333/);
		await expect(canvasOf(page, "device-key-0")).toHaveClass(/-m-\[2\.06rem\]/);
	});

	test("encoders on a non-touch device are 192px round canvases", async ({ page }) => {
		await openApp(page);
		const encoder = canvasOf(page, "device-encoder-0");
		await expect(encoder).toHaveClass(/rounded-full/);
		expect(await encoder.evaluate((node) => (node as HTMLCanvasElement).width)).toBe(192);
	});
});
