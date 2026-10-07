import { BOTH_ACTION, KEY_ACTION, openApp, test, expect } from "./fixtures/tauriMock";

test.describe("property inspector panel", () => {
	test.use({
		options: {
			seed: [
				{ controller: "Keypad", position: 1, action: KEY_ACTION },
				{ controller: "Keypad", position: 3, action: KEY_ACTION },
				{ controller: "Encoder", position: 0, action: BOTH_ACTION },
			],
		},
	});

	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("is hidden (but stays mounted) until a configured key is selected", async ({ page }) => {
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
		// Selecting an empty key does not open it.
		await page.getByTestId("device-key-0").getByTestId("key-canvas").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
	});

	test("opens for a configured key and tells the backend which instance is inspected", async ({ page, tauri }) => {
		await page.getByTestId("device-key-1").getByTestId("key-canvas").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		await expect(page.getByTestId("device-key-1").getByTestId("key-canvas")).toHaveClass(/outline-solid/);
		await expect.poll(async () => (await tauri.calls("switch_property_inspector")).at(-1)?.args).toEqual({ old: null, new: "sd-TEST.Default.Keypad.1.0" });
	});

	test("renders one property inspector iframe per configured action and shows only the selected one", async ({ page }) => {
		await page.getByTestId("device-key-1").getByTestId("key-canvas").click();
		const frames = page.getByTestId("property-inspector-panel").locator('iframe[title="Property inspector"]');
		await expect(frames).toHaveCount(2);
		await expect(frames.nth(0)).toBeVisible();
		await expect(frames.nth(1)).toBeHidden();
		await expect(frames.nth(0)).toHaveAttribute("name", "sd-TEST.Default.Keypad.1.0");
		await expect(frames.nth(0)).toHaveAttribute("src", /pi\/key\.html\|opendeck_property_inspector/);
	});

	test("selecting another key swaps the visible iframe without recreating the panel", async ({ page, tauri }) => {
		await page.getByTestId("device-key-1").getByTestId("key-canvas").click();
		const panel = page.getByTestId("property-inspector-panel");
		await panel.evaluate((node) => ((node as HTMLElement & { __marker?: string }).__marker = "same-panel"));
		const firstFrame = await panel.locator("iframe").first().elementHandle();

		await page.getByTestId("device-key-3").getByTestId("key-canvas").click();
		await expect(panel.locator('iframe[name="sd-TEST.Default.Keypad.3.0"]')).toBeVisible();
		await expect(panel.locator('iframe[name="sd-TEST.Default.Keypad.1.0"]')).toBeHidden();
		expect(await panel.evaluate((node) => (node as HTMLElement & { __marker?: string }).__marker)).toBe("same-panel");
		// The iframe element created for key 1 is the same DOM node (not reloaded).
		expect(await firstFrame!.evaluate((node) => node.isConnected)).toBe(true);
		await expect
			.poll(async () => (await tauri.calls("switch_property_inspector")).at(-1)?.args)
			.toEqual({
				old: "sd-TEST.Default.Keypad.1.0",
				new: "sd-TEST.Default.Keypad.3.0",
			});
	});

	test("a dial-controller instance without a property inspector opens an empty panel", async ({ page }) => {
		await page.getByTestId("device-touch-0").getByTestId("key-canvas").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		await expect(page.getByTestId("property-inspector-panel").locator("iframe").filter({ visible: true })).toHaveCount(0);
	});

	test("clicking the device background clears the selection", async ({ page, tauri }) => {
		await page.getByTestId("device-key-1").getByTestId("key-canvas").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		await page.getByTestId("device-view").click({ position: { x: 3, y: 3 } });
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
		await expect.poll(async () => (await tauri.calls("switch_property_inspector")).at(-1)?.args).toEqual({ old: "sd-TEST.Default.Keypad.1.0", new: null });
	});

	test("deleting the inspected instance closes the panel", async ({ page }) => {
		await page.getByTestId("device-key-1").getByTestId("key-canvas").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		await page.getByTestId("device-key-1").getByTestId("key-canvas").click({ button: "right" });
		await page.getByTestId("context-menu-delete").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
	});
});
