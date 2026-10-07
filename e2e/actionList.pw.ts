import { openApp, test, expect } from "./fixtures/tauriMock";

test.describe("action list", () => {
	test.beforeEach(async ({ page }) => {
		await openApp(page);
		await expect(page.getByTestId("action-item").first()).toBeVisible();
	});

	const names = (page: import("@playwright/test").Page) => page.getByTestId("action-item").locator("span").allTextContents();

	test("defaults to the Keys tab showing Keypad actions only", async ({ page }) => {
		await expect(page.getByTestId("action-tab-keys")).toHaveAttribute("aria-selected", "true");
		await expect(page.getByTestId("action-tab-dials")).toHaveAttribute("aria-selected", "false");
		expect((await names(page)).sort()).toEqual(["Clock", "Hybrid Action", "Key Only Action"]);
	});

	test("the Dials tab filters by the Encoder controller", async ({ page }) => {
		await page.getByTestId("action-tab-dials").click();
		await expect(page.getByTestId("action-tab-dials")).toHaveAttribute("aria-selected", "true");
		expect((await names(page)).sort()).toEqual(["Dial Only Action", "Hybrid Action", "Volume"]);
		await page.getByTestId("action-tab-keys").click();
		expect((await names(page)).sort()).toEqual(["Clock", "Hybrid Action", "Key Only Action"]);
	});

	test("groups actions by category and hides empty categories", async ({ page }) => {
		await expect(page.getByTestId("action-category")).toHaveCount(2);
		await expect(page.locator('[data-testid="action-category"][data-category="Test Plugin"] [data-testid="action-item"]')).toHaveCount(2);
		await page.getByTestId("action-search").fill("clock");
		await expect(page.getByTestId("action-category")).toHaveCount(1);
		await expect(page.getByTestId("action-category")).toHaveAttribute("data-category", "Utilities");
	});

	test("search filters by action name, case-insensitively", async ({ page }) => {
		await page.getByTestId("action-search").fill("HYBRID");
		expect(await names(page)).toEqual(["Hybrid Action"]);
		await page.getByTestId("action-search").fill("nothing-matches-this");
		await expect(page.getByTestId("action-item")).toHaveCount(0);
		await page.getByTestId("action-search").fill("");
		await expect(page.getByTestId("action-item")).toHaveCount(3);
	});

	test("search matching a category name keeps all of its actions for the active controller", async ({ page }) => {
		await page.getByTestId("action-search").fill("utilities");
		expect(await names(page)).toEqual(["Clock"]);
		await page.getByTestId("action-tab-dials").click();
		expect(await names(page)).toEqual(["Volume"]);
	});

	test("search applies together with the controller tab", async ({ page }) => {
		await page.getByTestId("action-search").fill("dial");
		await expect(page.getByTestId("action-item")).toHaveCount(0);
		await page.getByTestId("action-tab-dials").click();
		expect(await names(page)).toEqual(["Dial Only Action"]);
	});

	test("action items are draggable and expose their uuid", async ({ page }) => {
		const item = page.locator('[data-testid="action-item"][data-action-uuid="test.plugin.key"]');
		await expect(item).toHaveAttribute("draggable", "true");
		await expect(item).toHaveAttribute("title", "Key Only Action tooltip");
	});

	test("dragstart writes the namespaced MIME type and the legacy payload", async ({ page }) => {
		const stored = await page.locator('[data-testid="action-item"][data-action-uuid="test.plugin.key"]').evaluate((node) => {
			const transfer = new DataTransfer();
			node.dispatchEvent(new DragEvent("dragstart", { bubbles: true, cancelable: true, dataTransfer: transfer }));
			return { mime: transfer.getData("application/x-opendeck-action"), legacy: transfer.getData("action") };
		});
		expect(stored.legacy).toBe(stored.mime);
		expect(JSON.parse(stored.mime)).toMatchObject({ uuid: "test.plugin.key", controllers: ["Keypad"] });
	});
});
