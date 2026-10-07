import type { Page } from "@playwright/test";

import { openApp, test, expect } from "./fixtures/tauriMock";

const openPlugins = async (page: Page) => {
	await page.getByTestId("plugins-open").click();
	await expect(page.getByTestId("plugin-manager")).toBeVisible();
};

const plugin = (page: Page, name: string) => page.locator(`[data-testid="plugin-item"][data-plugin-name="${name}"]`);

test.describe("PluginManager", () => {
	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("lists installed plugins, builtin first", async ({ page }) => {
		await openPlugins(page);
		const installed = page.getByTestId("plugins-installed").getByTestId("plugin-item");
		await expect(installed).toHaveCount(3);
		await expect(installed.first()).toHaveAttribute("data-plugin-name", "Test Plugin");
		await expect(installed.nth(1)).toHaveAttribute("data-plugin-name", "Broken Plugin");
		await expect(installed.nth(2)).toHaveAttribute("data-plugin-name", "Utility Pack");
	});

	test("shows the store and archive catalogues", async ({ page }) => {
		await openPlugins(page);
		await expect(page.getByTestId("plugins-store").getByTestId("plugin-item")).toHaveCount(2);
		await expect(page.getByTestId("plugins-archive").getByTestId("plugin-item")).toHaveCount(1);
		await expect(plugin(page, "Store Alpha")).toContainText("Alice");
	});

	test("search filters the store and archive but not installed plugins", async ({ page }) => {
		await openPlugins(page);
		await page.getByTestId("plugins-search").fill("alpha");
		await expect(plugin(page, "Store Alpha")).toBeVisible();
		await expect(plugin(page, "Store Beta")).toBeHidden();
		await expect(plugin(page, "Archive Gamma")).toBeHidden();
		await expect(plugin(page, "Utility Pack")).toBeVisible();

		await page.getByTestId("plugins-search").fill("GAMMA");
		await expect(plugin(page, "Archive Gamma")).toBeVisible();
		await expect(plugin(page, "Store Alpha")).toBeHidden();

		await page.getByTestId("plugins-search").fill("");
		await expect(plugin(page, "Store Beta")).toBeVisible();
	});

	test("closes with the header button and Escape", async ({ page }) => {
		await openPlugins(page);
		await page.getByTestId("plugin-manager-header-close").click();
		await expect(page.getByTestId("plugin-manager")).toHaveCount(0);
		await openPlugins(page);
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("plugin-manager")).toHaveCount(0);
	});

	test("removing a plugin asks for confirmation and calls remove_plugin", async ({ page, tauri }) => {
		await openPlugins(page);
		await plugin(page, "Utility Pack").getByTestId("plugin-action").click();
		await expect.poll(async () => (await tauri.calls("remove_plugin")).length).toBe(1);
		expect((await tauri.calls("plugin:dialog|message")).some((call) => call.args.buttons == "YesNo")).toBe(true);
		expect((await tauri.calls("remove_plugin"))[0]!.args).toEqual({ id: "test.util" });
		await expect(plugin(page, "Utility Pack")).toHaveCount(0);
	});

	test("builtin plugins offer no removal action icon", async ({ page }) => {
		await openPlugins(page);
		await expect(plugin(page, "Test Plugin").getByTestId("plugin-action")).toHaveText("");
	});

	test("secondary actions open the settings interface or the log directory", async ({ page, tauri }) => {
		await openPlugins(page);
		await plugin(page, "Utility Pack").getByTestId("plugin-secondary-action").click();
		await plugin(page, "Broken Plugin").getByTestId("plugin-secondary-action").click();
		await expect.poll(async () => (await tauri.calls("show_settings_interface")).length).toBe(1);
		expect((await tauri.calls("show_settings_interface"))[0]!.args).toEqual({ plugin: "test.util" });
		await expect.poll(async () => (await tauri.calls("open_log_directory")).length).toBe(1);
	});

	test("clicking a store plugin opens its details", async ({ page }) => {
		await openPlugins(page);
		await plugin(page, "Store Alpha").getByTestId("plugin-action").click();
		await expect(page.getByTestId("plugin-details-close")).toBeVisible();
		await page.getByTestId("plugin-details-close").click();
		await expect(page.getByTestId("plugin-details-close")).toHaveCount(0);
	});
});
