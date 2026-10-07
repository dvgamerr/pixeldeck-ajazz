import type { Page } from "@playwright/test";

import { openApp, test, expect } from "../fixtures/tauriMock";

const plugin = (page: Page, name: string) => page.locator(`[data-testid="plugin-item"][data-plugin-name="${name}"]`);

test.describe("Popup and PopupHeader", () => {
	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("popups render nothing while closed", async ({ page }) => {
		for (const id of ["plugin-manager", "profile-manager", "settings-popup", "application-profiles-popup"]) await expect(page.getByTestId(id)).toHaveCount(0);
	});

	test.describe("full-screen variant (plugin manager)", () => {
		test.beforeEach(async ({ page }) => {
			await page.getByTestId("plugins-open").click();
			await expect(page.getByTestId("plugin-manager")).toBeVisible();
		});

		test("is a modal dialog covering the viewport", async ({ page }) => {
			const popup = page.getByTestId("plugin-manager");
			await expect(popup).toHaveAttribute("role", "dialog");
			await expect(popup).toHaveAttribute("aria-modal", "true");
			await expect(popup).toHaveClass(/modal-open/);
			const box = (await page.getByTestId("plugin-manager").locator("section.modal-box").boundingBox())!;
			const viewport = page.viewportSize()!;
			expect(box.width).toBeGreaterThan(viewport.width - 40);
			expect(box.height).toBeGreaterThan(viewport.height - 40);
		});

		test("uses the bordered header with eyebrow, title and a labelled close button", async ({ page }) => {
			const header = page.getByTestId("plugin-manager-header");
			await expect(header).toHaveClass(/border-b/);
			await expect(header.locator(".ui-eyebrow")).toHaveText("PixelDeck");
			await expect(header.getByRole("heading", { name: "Manage plugins" })).toBeVisible();
			const close = page.getByTestId("plugin-manager-header-close");
			await expect(close).toHaveAccessibleName("Close plugin manager");
			await expect(close).not.toHaveClass(/btn-sm/);
		});

		test("the header close button closes the popup", async ({ page }) => {
			await page.getByTestId("plugin-manager-header-close").click();
			await expect(page.getByTestId("plugin-manager")).toHaveCount(0);
		});

		test("clicking the backdrop closes the popup", async ({ page }) => {
			await page.getByRole("button", { name: "Close modal" }).click({ position: { x: 3, y: 3 } });
			await expect(page.getByTestId("plugin-manager")).toHaveCount(0);
		});

		test("clicking inside the content keeps it open", async ({ page }) => {
			await page.getByRole("heading", { name: "Installed plugins" }).click();
			await expect(page.getByTestId("plugin-manager")).toBeVisible();
		});

		test("the popup stacks above the sidebar", async ({ page }) => {
			const topmost = await page.evaluate(() => {
				const sidebar = document.querySelector('[data-testid="sidebar"]')!.getBoundingClientRect();
				const hit = document.elementFromPoint(sidebar.x + 20, sidebar.y + 20)!;
				return !!hit.closest('[data-testid="plugin-manager"]');
			});
			expect(topmost).toBe(true);
		});
	});

	test.describe("inline variant (profile manager)", () => {
		test.beforeEach(async ({ page }) => {
			await page.getByTestId("profile-selector").selectOption("opendeck_edit_profiles");
			await expect(page.getByTestId("profile-manager")).toBeVisible();
		});

		test("is an absolutely positioned card inside the sidebar, not a full-screen modal", async ({ page }) => {
			const popup = page.getByTestId("profile-manager");
			await expect(popup).toHaveCSS("position", "absolute");
			await expect(popup).not.toHaveClass(/modal/);
			await expect(page.getByRole("button", { name: "Close modal" })).toHaveCount(0);
			const info = await popup.evaluate((node) => ({ role: node.getAttribute("role"), modal: node.getAttribute("aria-modal"), tag: node.tagName }));
			expect(info).toEqual({ role: "dialog", modal: "true", tag: "SECTION" });
		});

		test("uses the compact header with the device name as title", async ({ page }) => {
			const header = page.getByTestId("profile-manager-header");
			await expect(header).toHaveClass(/mb-3/);
			await expect(header).not.toHaveClass(/border-b/);
			await expect(header.locator(".ui-eyebrow")).toHaveText("Profiles");
			await expect(header.getByRole("heading")).toHaveText("Ajazz AKP05E_552A");
			const close = page.getByTestId("profile-manager-header-close");
			await expect(close).toHaveClass(/btn-sm/);
			await expect(close).toHaveAccessibleName("Close profile manager");
		});

		test("the header close button closes it", async ({ page }) => {
			await page.getByTestId("profile-manager-header-close").click();
			await expect(page.getByTestId("profile-manager")).toHaveCount(0);
		});
	});

	test("the settings popup header exposes its own close button", async ({ page }) => {
		await page.getByTestId("settings-open").click();
		await expect(page.getByTestId("settings-popup")).toBeVisible();
		await expect(page.getByTestId("settings-header-close")).toBeVisible();
		await page.getByTestId("settings-header-close").click();
		await expect(page.getByTestId("settings-popup")).toHaveCount(0);
	});
});

test.describe("ListedPlugin and Tooltip", () => {
	test.describe("normal mode", () => {
		test.beforeEach(async ({ page }) => {
			await openApp(page);
			await page.getByTestId("plugins-open").click();
			await expect(page.getByTestId("plugin-manager")).toBeVisible();
		});

		test("shows icon, name and version as title and subtitle", async ({ page }) => {
			const item = plugin(page, "Utility Pack");
			await expect(item).toContainText("2.3.1");
			await expect(item.getByRole("img", { name: "Utility Pack" })).toHaveAttribute("src", "http://127.0.0.1:57118/util.png");
			await expect(item.getByRole("img", { name: "Utility Pack" })).toHaveAttribute("loading", "lazy");
		});

		test("unregistered plugins are dimmed, registered ones are not", async ({ page }) => {
			await expect(plugin(page, "Broken Plugin").locator("img")).toHaveClass(/opacity-50/);
			await expect(plugin(page, "Utility Pack").locator("img")).not.toHaveClass(/opacity-50/);
		});

		test("the primary button removes a third-party plugin, never a builtin one's icon", async ({ page, tauri }) => {
			await plugin(page, "Utility Pack").getByTestId("plugin-action").click();
			await expect.poll(async () => (await tauri.calls("remove_plugin")).length).toBe(1);
			expect((await tauri.calls("remove_plugin"))[0]!.args).toEqual({ id: "test.util" });
			await expect(plugin(page, "Utility Pack")).toHaveCount(0);
			await expect(page.locator(".ui-section-heading", { hasText: "Installed plugins" }).locator(".badge")).toHaveText("2");
		});

		test("the secondary button does nothing for a healthy plugin without a settings interface", async ({ page, tauri }) => {
			await plugin(page, "Test Plugin").getByTestId("plugin-secondary-action").click();
			await page.waitForTimeout(200);
			expect(await tauri.calls("show_settings_interface")).toHaveLength(0);
			expect(await tauri.calls("open_log_directory")).toHaveLength(0);
		});

		test("store plugins only offer a primary action (no secondary button)", async ({ page }) => {
			await expect(plugin(page, "Store Alpha").getByTestId("plugin-secondary-action")).toHaveCount(0);
			await expect(plugin(page, "Store Alpha").getByTestId("plugin-action")).toBeVisible();
		});

		test("the store section tooltip reveals its explanation on hover", async ({ page }) => {
			const tooltip = page.getByRole("tooltip").filter({ hasText: "Open-source plugins downloaded from the author's releases." });
			await expect(tooltip).toBeHidden();
			await page.getByRole("button", { name: "More information" }).first().hover();
			await expect(tooltip).toBeVisible();
			await page.getByRole("heading", { name: "Installed plugins" }).hover();
			await expect(tooltip).toBeHidden();
		});
	});

	test.describe("developer mode", () => {
		test.use({ options: { settings: { developer: true } } });

		test.beforeEach(async ({ page }) => {
			await openApp(page);
			await page.getByTestId("plugins-open").click();
			await expect(page.getByTestId("plugin-manager")).toBeVisible();
		});

		test("the primary button reloads the plugin instead of removing it, builtin plugins included", async ({ page, tauri }) => {
			await plugin(page, "Utility Pack").getByTestId("plugin-action").click();
			await plugin(page, "Test Plugin").getByTestId("plugin-action").click();
			await expect.poll(async () => (await tauri.calls("reload_plugin")).length).toBe(2);
			expect((await tauri.calls("reload_plugin")).map((call) => call.args.id)).toEqual(["test.util", "test.plugin"]);
			expect(await tauri.calls("remove_plugin")).toHaveLength(0);
			await expect(plugin(page, "Utility Pack")).toHaveCount(1);
		});

		test("every installed plugin gets an icon on its primary button", async ({ page }) => {
			for (const name of ["Utility Pack", "Test Plugin", "Broken Plugin"]) await expect(plugin(page, name).getByTestId("plugin-action").locator("svg")).toHaveCount(1);
		});
	});
});
