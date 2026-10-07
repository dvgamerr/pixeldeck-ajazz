import type { Page } from "@playwright/test";

import { openApp, test, expect } from "./fixtures/tauriMock";

const openSettings = async (page: Page) => {
	await page.getByTestId("settings-open").click();
	await expect(page.getByTestId("settings-popup")).toBeVisible();
};

const lastSettings = async (tauri: { calls: (cmd: string) => Promise<{ args: { settings: Record<string, unknown> } }[]> }) => {
	const calls = await tauri.calls("set_settings");
	return calls[calls.length - 1]!.args.settings;
};

test.describe("Settings", () => {
	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("loads persisted settings into the controls", async ({ page }) => {
		await openSettings(page);
		await expect(page.getByTestId("settings-darktheme")).toBeChecked();
		await expect(page.getByTestId("settings-updatecheck")).toBeChecked();
		await expect(page.getByTestId("settings-background")).not.toBeChecked();
		await expect(page.getByTestId("settings-developer")).not.toBeChecked();
		await expect(page.getByTestId("settings-brightness")).toHaveValue("50");
		await expect(page.getByTestId("settings-language")).toHaveValue("en");
	});

	for (const key of ["background", "autolaunch", "updatecheck", "statistics", "separatewine", "developer", "disabledevices", "darktheme"]) {
		test(`toggling ${key} persists it through set_settings`, async ({ page, tauri }) => {
			await openSettings(page);
			const toggle = page.getByTestId(`settings-${key}`);
			const before = await toggle.isChecked();
			await toggle.click();
			await expect.poll(async () => (await lastSettings(tauri))[key]).toBe(!before);
			await toggle.click();
			await expect.poll(async () => (await lastSettings(tauri))[key]).toBe(before);
		});
	}

	test("the dark theme toggle updates the document theme", async ({ page }) => {
		await openSettings(page);
		await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
		await page.getByTestId("settings-darktheme").click();
		await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
	});

	test("changing language reloads localisations", async ({ page, tauri }) => {
		await openSettings(page);
		await page.getByTestId("settings-language").selectOption("ja");
		await expect.poll(async () => (await lastSettings(tauri)).language).toBe("ja");
		await expect.poll(async () => (await tauri.calls("get_localisations")).at(-1)?.args).toEqual({ locale: "ja" });
	});

	test("changing brightness persists it", async ({ page, tauri }) => {
		await openSettings(page);
		await page.getByTestId("settings-brightness").fill("80");
		await expect.poll(async () => (await lastSettings(tauri)).brightness).toBe(80);
	});

	test("device_brightness events adjust and clamp the brightness", async ({ page, tauri }) => {
		await expect.poll(async () => await tauri.listenerCount("device_brightness")).toBe(1);
		await tauri.emit("device_brightness", { action: "increase", value: 10 });
		await expect.poll(async () => (await lastSettings(tauri)).brightness).toBe(60);
		await tauri.emit("device_brightness", { action: "decrease", value: 200 });
		await expect.poll(async () => (await lastSettings(tauri)).brightness).toBe(0);
		await tauri.emit("device_brightness", { action: "set", value: 33 });
		await expect.poll(async () => (await lastSettings(tauri)).brightness).toBe(33);
		await openSettings(page);
		await expect(page.getByTestId("settings-brightness")).toHaveValue("33");
	});

	test("directory buttons call the backend", async ({ page, tauri }) => {
		await openSettings(page);
		await page.getByTestId("settings-open-config").click();
		await page.getByTestId("settings-open-logs").click();
		await expect.poll(async () => (await tauri.calls("open_config_directory")).length).toBe(1);
		await expect.poll(async () => (await tauri.calls("open_log_directory")).length).toBe(1);
	});

	test("the startup image tab explains when the device has no startup image support", async ({ page }) => {
		await openSettings(page);
		await page.getByTestId("settings-tab-startup-image").click();
		await expect(page.getByText("No supported device selected")).toBeVisible();
		await page.getByTestId("settings-tab-general").click();
		await expect(page.getByTestId("settings-darktheme")).toBeVisible();
	});

	test("closing settings reloads the selected profile and resumes rendering", async ({ page, tauri }) => {
		await openSettings(page);
		await page.getByTestId("settings-header-close").click();
		await expect(page.getByTestId("settings-popup")).toHaveCount(0);
		const [call] = await tauri.calls("reload_selected_profile");
		expect(call!.args).toEqual({ device: "sd-TEST" });

		await openSettings(page);
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("settings-popup")).toHaveCount(0);
	});
});
