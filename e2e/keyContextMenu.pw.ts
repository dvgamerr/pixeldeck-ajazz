import { contextMenuAt, KEY_ACTION, makeDataTransfer, openApp, test, expect } from "./fixtures/tauriMock";

test.describe("key context menu", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 2, action: KEY_ACTION }] } });

	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("opens at the click position and is fixed-positioned", async ({ page }) => {
		// Record where the browser really delivered the event; the layout can still be settling after load.
		await page.evaluate(() => {
			document.addEventListener("contextmenu", (event) => ((window as unknown as { __ctx: number[] }).__ctx = [event.clientX, event.clientY]), { capture: true });
		});
		await page
			.getByTestId("device-key-0")
			.getByTestId("key-canvas")
			.click({ button: "right", position: { x: 40, y: 30 } });
		const [x, y] = await page.evaluate(() => (window as unknown as { __ctx: number[] }).__ctx);

		const menu = page.getByTestId("key-context-menu");
		await expect(menu).toBeVisible();
		await expect(menu).toHaveCSS("position", "fixed");
		const menuBox = (await menu.boundingBox())!;
		expect(Math.round(menuBox.x)).toBe(x);
		expect(Math.round(menuBox.y)).toBe(y);
		// Empty slot: only paste is offered.
		await expect(page.getByTestId("context-menu-paste")).toBeVisible();
		await expect(page.getByTestId("context-menu-edit")).toHaveCount(0);
		await expect(page.getByTestId("context-menu-delete")).toHaveCount(0);
	});

	test("occupied slot offers edit, copy and delete", async ({ page }) => {
		await page.getByTestId("device-key-2").getByTestId("key-canvas").click({ button: "right" });
		await expect(page.getByTestId("context-menu-edit")).toBeVisible();
		await expect(page.getByTestId("context-menu-copy")).toBeVisible();
		await expect(page.getByTestId("context-menu-delete")).toBeVisible();
		await expect(page.getByTestId("context-menu-paste")).toHaveCount(0);
	});

	test("is clamped inside the viewport vertically near the bottom edge", async ({ page }) => {
		const viewport = page.viewportSize()!;
		await contextMenuAt(page, "device-key-4", 100, viewport.height - 2);
		const box = (await page.getByTestId("key-context-menu").boundingBox())!;
		expect(box.y).toBeGreaterThanOrEqual(0);
		expect(box.y + box.height).toBeLessThanOrEqual(viewport.height);
	});

	test("is clamped inside the viewport horizontally near the right edge", async ({ page }) => {
		const viewport = page.viewportSize()!;
		await contextMenuAt(page, "device-key-4", viewport.width - 2, 100);
		const box = (await page.getByTestId("key-context-menu").boundingBox())!;
		expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
	});

	test("is clamped to the top-left corner", async ({ page }) => {
		await contextMenuAt(page, "device-key-0", 0, 0);
		const box = (await page.getByTestId("key-context-menu").boundingBox())!;
		expect(box.x).toBeGreaterThanOrEqual(0);
		expect(box.y).toBeGreaterThanOrEqual(0);
	});

	test("the native context menu is suppressed", async ({ page }) => {
		const prevented = await page.evaluate(() => {
			const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: 5, clientY: 5 });
			document.body.dispatchEvent(event);
			return event.defaultPrevented;
		});
		expect(prevented).toBe(true);
	});

	test("closes when clicking elsewhere", async ({ page }) => {
		await page.getByTestId("device-key-0").getByTestId("key-canvas").click({ button: "right" });
		await expect(page.getByTestId("key-context-menu")).toBeVisible();
		await page.getByTestId("sidebar").click({ position: { x: 5, y: 5 } });
		await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
	});

	test("closes when a key drag starts", async ({ page }) => {
		const source = page.getByTestId("device-key-2").getByTestId("key-canvas");
		await source.click({ button: "right" });
		await expect(page.getByTestId("key-context-menu")).toBeVisible();
		const transfer = await makeDataTransfer(page, {});
		await source.dispatchEvent("dragstart", { dataTransfer: transfer });
		await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
	});

	test("closes when an action library drag starts", async ({ page }) => {
		await page.getByTestId("device-key-0").getByTestId("key-canvas").click({ button: "right" });
		await expect(page.getByTestId("key-context-menu")).toBeVisible();
		const transfer = await makeDataTransfer(page, {});
		await page.getByTestId("action-item").first().dispatchEvent("dragstart", { dataTransfer: transfer });
		await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
	});

	test("delete removes the instance through the backend", async ({ page, tauri }) => {
		const key = page.getByTestId("device-key-2");
		await expect(key).toHaveAttribute("data-occupied", "true");
		await key.getByTestId("key-canvas").click({ button: "right" });
		await page.getByTestId("context-menu-delete").click();
		await expect(key).toHaveAttribute("data-occupied", "false");
		const calls = await tauri.calls("remove_instance");
		expect(calls).toHaveLength(1);
		expect(calls[0]!.args.context).toBe("sd-TEST.Default.Keypad.2.0");
	});

	test("copy then paste moves a retained copy to another slot", async ({ page, tauri }) => {
		await page.getByTestId("device-key-2").getByTestId("key-canvas").click({ button: "right" });
		await page.getByTestId("context-menu-copy").click();
		await page.getByTestId("device-key-7").getByTestId("key-canvas").click({ button: "right" });
		await page.getByTestId("context-menu-paste").click();

		await expect(page.getByTestId("device-key-7")).toHaveAttribute("data-occupied", "true");
		await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "true");
		const [call] = await tauri.calls("move_instance");
		expect(call!.args).toMatchObject({
			source: { device: "sd-TEST", profile: "Default", controller: "Keypad", position: 2 },
			destination: { device: "sd-TEST", profile: "Default", controller: "Keypad", position: 7 },
			retain: true,
		});
	});

	test("edit opens the instance editor", async ({ page, tauri }) => {
		await page.getByTestId("device-key-2").getByTestId("key-canvas").click({ button: "right" });
		await page.getByTestId("context-menu-edit").click();
		await expect(page.getByTestId("instance-editor")).toBeVisible();
		await expect.poll(async () => (await tauri.calls("set_state")).length).toBeGreaterThan(0);
		await page.getByTestId("instance-editor-done").click();
		await expect(page.getByTestId("instance-editor")).toHaveCount(0);
	});
});
