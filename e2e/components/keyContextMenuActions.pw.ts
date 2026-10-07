import type { Page } from "@playwright/test";

import { DIAL_ACTION, KEY_ACTION, openApp, test, expect } from "../fixtures/tauriMock";

const rightClick = (page: Page, testId: string) => page.getByTestId(testId).getByTestId("key-canvas").click({ button: "right" });

test.describe("KeyContextMenu actions", () => {
	test.use({
		options: {
			seed: [
				{ controller: "Keypad", position: 2, action: KEY_ACTION },
				{ controller: "Encoder", position: 1, action: DIAL_ACTION },
			],
		},
	});

	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("the menu element is portalled to document.body", async ({ page }) => {
		await rightClick(page, "device-key-2");
		const parent = await page.getByTestId("key-context-menu").evaluate((node) => node.parentElement === document.body);
		expect(parent).toBe(true);
	});

	test("an occupied slot lists Edit, Copy, Delete in that order with labels", async ({ page }) => {
		await rightClick(page, "device-key-2");
		const items = page.getByTestId("key-context-menu").locator("button");
		await expect(items).toHaveText(["Edit", "Copy", "Delete"]);
		await expect(page.getByTestId("context-menu-delete")).toHaveClass(/text-error/);
	});

	test("an empty slot offers only Paste", async ({ page }) => {
		await rightClick(page, "device-key-3");
		await expect(page.getByTestId("key-context-menu").locator("button")).toHaveText(["Paste"]);
	});

	test("only one menu is open at a time: a second right click moves it", async ({ page }) => {
		await rightClick(page, "device-key-2");
		await expect(page.getByTestId("context-menu-edit")).toBeVisible();
		// key 9 is on the other row, away from the first menu.
		await rightClick(page, "device-key-9");
		await expect(page.getByTestId("key-context-menu")).toHaveCount(1);
		await expect(page.getByTestId("context-menu-paste")).toBeVisible();
		await expect(page.getByTestId("context-menu-edit")).toHaveCount(0);
	});

	test("every action closes the menu", async ({ page }) => {
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-copy").click();
		await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-edit").click();
		await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
	});

	test("Delete removes only that slot and sends no other mutation", async ({ page, tauri }) => {
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-delete").click();
		await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "false");
		await expect(page.getByTestId("device-touch-1")).toHaveAttribute("data-occupied", "true");
		expect((await tauri.calls("remove_instance")).map((call) => call.args)).toEqual([{ context: "sd-TEST.Default.Keypad.2.0" }]);
		expect(await tauri.calls("move_instance")).toHaveLength(0);
		expect(await tauri.calls("create_instance")).toHaveLength(0);
	});

	test("Delete on an encoder zone removes the Encoder instance", async ({ page, tauri }) => {
		await rightClick(page, "device-touch-1");
		await page.getByTestId("context-menu-delete").click();
		await expect(page.getByTestId("device-touch-1")).toHaveAttribute("data-occupied", "false");
		expect((await tauri.calls("remove_instance"))[0]!.args).toEqual({ context: "sd-TEST.Default.Encoder.1.0" });
	});

	test("Delete closes the property inspector when the deleted key was inspected", async ({ page }) => {
		await page.getByTestId("device-key-2").getByTestId("key-canvas").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-delete").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
	});

	test("Delete also closes the instance editor that was open for that key", async ({ page }) => {
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-edit").click();
		await expect(page.getByTestId("instance-editor")).toBeVisible();
		// The editor modal covers the page; delete through the (still portalled) menu via a synthetic contextmenu event.
		await page.getByTestId("device-key-2").getByTestId("key-canvas").dispatchEvent("contextmenu", { clientX: 300, clientY: 300 });
		await page.getByTestId("context-menu-delete").dispatchEvent("click");
		await expect(page.getByTestId("instance-editor")).toHaveCount(0);
		await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "false");
	});

	test("Paste with nothing copied does nothing", async ({ page, tauri }) => {
		await rightClick(page, "device-key-3");
		await page.getByTestId("context-menu-paste").click();
		await expect(page.getByTestId("device-key-3")).toHaveAttribute("data-occupied", "false");
		expect(await tauri.calls("move_instance")).toHaveLength(0);
	});

	test("Copy does not call the backend by itself", async ({ page, tauri }) => {
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-copy").click();
		expect(await tauri.calls("move_instance")).toHaveLength(0);
		expect(await tauri.calls("create_instance")).toHaveLength(0);
	});

	test("a copied key can be pasted into several empty slots", async ({ page, tauri }) => {
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-copy").click();
		for (const slot of [5, 8]) {
			await rightClick(page, `device-key-${slot}`);
			await page.getByTestId("context-menu-paste").click();
			await expect(page.getByTestId(`device-key-${slot}`)).toHaveAttribute("data-occupied", "true");
		}
		const calls = await tauri.calls("move_instance");
		expect(calls.map((call) => [call.args.source.position, call.args.destination.position, call.args.retain])).toEqual([
			[2, 5, true],
			[2, 8, true],
		]);
		await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "true");
	});

	test("an encoder zone can be copied to another zone", async ({ page, tauri }) => {
		await rightClick(page, "device-touch-1");
		await page.getByTestId("context-menu-copy").click();
		await rightClick(page, "device-touch-3");
		await page.getByTestId("context-menu-paste").click();
		await expect(page.getByTestId("device-touch-3")).toHaveAttribute("data-occupied", "true");
		const [call] = await tauri.calls("move_instance");
		expect(call!.args).toMatchObject({
			source: { device: "sd-TEST", profile: "Default", controller: "Encoder", position: 1 },
			destination: { device: "sd-TEST", profile: "Default", controller: "Encoder", position: 3 },
			retain: true,
		});
	});

	test("pasting a key onto a touch zone is rejected by the backend and leaves the zone empty", async ({ page, tauri }) => {
		const errors: string[] = [];
		page.on("pageerror", (error) => errors.push(error.message));
		await rightClick(page, "device-key-2");
		await page.getByTestId("context-menu-copy").click();
		await rightClick(page, "device-touch-0");
		await page.getByTestId("context-menu-paste").click();
		await expect.poll(async () => (await tauri.calls("move_instance")).length).toBe(1);
		await expect(page.getByTestId("device-touch-0")).toHaveAttribute("data-occupied", "false");
		await expect(page.getByTestId("device-key-2")).toHaveAttribute("data-occupied", "true");
		// The rejection is currently not caught anywhere (no user feedback). Documented in docs/E2E_COVERAGE.md.
		expect(errors.length).toBeLessThanOrEqual(1);
	});

	test("Edit opens the editor for the right-clicked instance only", async ({ page, tauri }) => {
		await rightClick(page, "device-touch-1");
		await page.getByTestId("context-menu-edit").click();
		await expect(page.getByTestId("instance-editor")).toBeVisible();
		await expect(page.getByTestId("instance-editor")).toHaveCount(1);
		await expect.poll(async () => (await tauri.calls("set_state")).at(-1)?.args.instance.context).toBe("sd-TEST.Default.Encoder.1.0");
	});

	test("the menu does not open on the sidebar or other page areas", async ({ page }) => {
		await page.getByTestId("sidebar").click({ button: "right", position: { x: 20, y: 20 } });
		await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
	});

	test("opening the menu does not select the key", async ({ page }) => {
		await rightClick(page, "device-key-2");
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
	});
});
