import type { Page } from "@playwright/test";

import { openApp, test, expect } from "../fixtures/tauriMock";

const DEVICE = "sd-TEST";

const openMapping = async (page: Page) => {
	await page.getByTestId("profile-selector").selectOption("opendeck_edit_profiles");
	await page.getByTestId("profile-application-mapping").click();
	await expect(page.getByTestId("application-profiles-popup")).toBeVisible();
};

const rowOf = (page: Page, application: string) => page.locator(`[data-testid="application-profile-row"][data-application="${application}"]`);
const lastSaved = async (tauri: { calls: (cmd: string) => Promise<{ args: any }[]> }) => (await tauri.calls("set_application_profiles")).at(-1)?.args.value;

test.describe("ApplicationProfilesPopup", () => {
	test.use({ options: { applications: ["firefox", "code", "vlc"] } });

	test.beforeEach(async ({ page }) => {
		await openApp(page);
		await openMapping(page);
	});

	test("shows the device name, usage hint and starts without mappings", async ({ page }) => {
		const popup = page.getByTestId("application-profiles-popup");
		await expect(popup).toContainText("Application mapping");
		await expect(popup).toContainText("Ajazz AKP05E_552A");
		await expect(popup).toContainText("If an application is missing, switch to it and back");
		await expect(page.getByTestId("application-profile-row")).toHaveCount(0);
		await expect(page.getByTestId("application-profiles-error")).toHaveCount(0);
	});

	test("the application picker lists the default profile and every detected application", async ({ page }) => {
		const options = page.getByTestId("application-add-app").locator("option");
		await expect(options).toHaveText(["Select application...", "Default profile", "──────────", "firefox", "code", "vlc"]);
		await expect(options.nth(0)).toBeDisabled();
		await expect(options.nth(2)).toBeDisabled();
	});

	test("the profile picker lists the device profiles", async ({ page }) => {
		const options = page.getByTestId("application-add-profile").locator("option");
		await expect(options).toHaveText(["Select profile...", "Default", "Gaming"]);
	});

	test("choosing an application alone does not create a mapping", async ({ page, tauri }) => {
		await page.getByTestId("application-add-app").selectOption("firefox");
		await expect(page.getByTestId("application-profile-row")).toHaveCount(0);
		expect(await tauri.calls("set_application_profiles")).toHaveLength(0);
		await expect(page.getByTestId("application-add-app")).toHaveValue("firefox");
	});

	test("application then profile adds a row, persists it and resets the pickers", async ({ page, tauri }) => {
		await page.getByTestId("application-add-app").selectOption("firefox");
		await page.getByTestId("application-add-profile").selectOption("Gaming");
		const row = rowOf(page, "firefox");
		await expect(row).toBeVisible();
		await expect(row).toContainText("firefox:");
		await expect(row.getByTestId("application-profile-select")).toHaveValue("Gaming");
		await expect(page.getByTestId("application-add-app")).toHaveValue("opendeck_select_application");
		await expect(page.getByTestId("application-add-profile")).toHaveValue("opendeck_select_profile");
		await expect.poll(() => lastSaved(tauri)).toEqual({ firefox: { [DEVICE]: "Gaming" } });
	});

	test("profile then application works as well", async ({ page, tauri }) => {
		await page.getByTestId("application-add-profile").selectOption("Default");
		await page.getByTestId("application-add-app").selectOption("code");
		await expect(rowOf(page, "code").getByTestId("application-profile-select")).toHaveValue("Default");
		await expect.poll(() => lastSaved(tauri)).toEqual({ code: { [DEVICE]: "Default" } });
	});

	test("a mapped application disappears from the picker", async ({ page }) => {
		await page.getByTestId("application-add-app").selectOption("firefox");
		await page.getByTestId("application-add-profile").selectOption("Gaming");
		await expect(page.getByTestId("application-add-app").locator("option")).toHaveText(["Select application...", "Default profile", "──────────", "code", "vlc"]);
	});

	test("the default profile mapping is labelled and sorted first", async ({ page, tauri }) => {
		await page.getByTestId("application-add-app").selectOption("vlc");
		await page.getByTestId("application-add-profile").selectOption("Gaming");
		await page.getByTestId("application-add-app").selectOption("opendeck_default");
		await page.getByTestId("application-add-profile").selectOption("Default");
		const rows = page.getByTestId("application-profile-row");
		await expect(rows).toHaveCount(2);
		await expect(rows.first()).toContainText("Default profile:");
		await expect(rows.first()).toHaveAttribute("data-application", "opendeck_default");
		await expect(rows.last()).toHaveAttribute("data-application", "vlc");
		await expect(page.getByTestId("application-add-app").locator("option")).not.toContainText(["Default profile"]);
		await expect.poll(() => lastSaved(tauri)).toEqual({ opendeck_default: { [DEVICE]: "Default" }, vlc: { [DEVICE]: "Gaming" } });
	});

	test("rows are ordered alphabetically", async ({ page }) => {
		for (const [app, profile] of [
			["vlc", "Default"],
			["code", "Default"],
			["firefox", "Default"],
		] as const) {
			await page.getByTestId("application-add-app").selectOption(app);
			await page.getByTestId("application-add-profile").selectOption(profile);
		}
		await expect(page.getByTestId("application-profile-row")).toHaveCount(3);
		const order = await page.getByTestId("application-profile-row").evaluateAll((rows) => rows.map((row) => row.getAttribute("data-application")));
		expect(order).toEqual(["code", "firefox", "vlc"]);
	});

	test("changing the profile of an existing row persists the change", async ({ page, tauri }) => {
		await page.getByTestId("application-add-app").selectOption("firefox");
		await page.getByTestId("application-add-profile").selectOption("Default");
		await expect.poll(() => lastSaved(tauri)).toEqual({ firefox: { [DEVICE]: "Default" } });
		await rowOf(page, "firefox").getByTestId("application-profile-select").selectOption("Gaming");
		await expect.poll(() => lastSaved(tauri)).toEqual({ firefox: { [DEVICE]: "Gaming" } });
	});

	test("Remove application drops the mapping and offers the application again", async ({ page, tauri }) => {
		await page.getByTestId("application-add-app").selectOption("firefox");
		await page.getByTestId("application-add-profile").selectOption("Gaming");
		await expect(rowOf(page, "firefox")).toBeVisible();
		const select = rowOf(page, "firefox").getByTestId("application-profile-select");
		await expect(select.locator("option").last()).toHaveText("Remove application");
		await select.selectOption({ label: "Remove application" });
		await expect(rowOf(page, "firefox")).toHaveCount(0);
		await expect.poll(() => lastSaved(tauri)).toEqual({});
		await expect(page.getByTestId("application-add-app").locator("option")).toContainText(["firefox"]);
	});

	test("the applications event refreshes the picker", async ({ page, tauri }) => {
		await tauri.emit("applications", ["firefox", "gimp"]);
		await expect(page.getByTestId("application-add-app").locator("option")).toHaveText(["Select application...", "Default profile", "──────────", "firefox", "gimp"]);
	});

	test("a failed save is surfaced in the popup", async ({ page, tauri }) => {
		await tauri.fail("set_application_profiles", "disk full");
		await page.getByTestId("application-add-app").selectOption("firefox");
		await page.getByTestId("application-add-profile").selectOption("Gaming");
		await expect(page.getByTestId("application-profiles-error")).toHaveText("Unable to save application profiles: disk full");
		await tauri.fail("set_application_profiles", null);
		await page.getByTestId("application-add-app").selectOption("vlc");
		await page.getByTestId("application-add-profile").selectOption("Default");
		await expect(page.getByTestId("application-profiles-error")).toHaveCount(0);
	});

	test("closes with the header button, keeping the profile manager open", async ({ page }) => {
		await page.getByTestId("application-profiles-header-close").click();
		await expect(page.getByTestId("application-profiles-popup")).toHaveCount(0);
		await expect(page.getByTestId("profile-manager")).toBeVisible();
	});

	test("Escape closes the mapping first and the manager second", async ({ page }) => {
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("application-profiles-popup")).toHaveCount(0);
		await expect(page.getByTestId("profile-manager")).toBeVisible();
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("profile-manager")).toHaveCount(0);
	});

	test("deleting a profile removes the mappings that used it", async ({ page, tauri }) => {
		await page.getByTestId("application-add-app").selectOption("firefox");
		await page.getByTestId("application-add-profile").selectOption("Gaming");
		await expect.poll(() => lastSaved(tauri)).toEqual({ firefox: { [DEVICE]: "Gaming" } });
		await page.getByTestId("application-profiles-header-close").click();
		await page.locator('[data-testid="profile-item"][data-profile="Gaming"]').getByTestId("profile-delete").click();
		await expect(page.locator('[data-testid="profile-item"][data-profile="Gaming"]')).toHaveCount(0);
		await page.getByTestId("profile-application-mapping").click();
		await expect(page.getByTestId("application-profile-row")).toHaveCount(0);
		await expect(page.getByTestId("application-add-app").locator("option")).toContainText(["firefox"]);
	});
});

test.describe("ApplicationProfilesPopup with existing mappings", () => {
	test.use({ options: { applications: ["firefox", "code"], profiles: { [DEVICE]: ["Default", "Gaming", "Work/Dev", "Work/Ops"] } } });

	test.beforeEach(async ({ page }) => {
		await page.addInitScript(() => {
			(window as any).__mock.state.applicationProfiles = {
				firefox: { "sd-TEST": "Gaming" },
				code: { "sd-OTHER": "Default" },
				opendeck_default: { "sd-TEST": "Default", "sd-OTHER": "Default" },
				ghost: { "sd-TEST": "" },
			};
		});
		await openApp(page);
		await openMapping(page);
	});

	test("loads the saved mappings of this device only", async ({ page }) => {
		const apps = await page.getByTestId("application-profile-row").evaluateAll((rows) => rows.map((row) => row.getAttribute("data-application")));
		expect(apps).toEqual(["opendeck_default", "firefox"]);
		await expect(rowOf(page, "firefox").getByTestId("application-profile-select")).toHaveValue("Gaming");
		await expect(rowOf(page, "opendeck_default").getByTestId("application-profile-select")).toHaveValue("Default");
	});

	test("an application only mapped for another device is still available here", async ({ page }) => {
		await expect(page.getByTestId("application-add-app").locator("option")).toHaveText(["Select application...", "code"]);
	});

	test("empty assignments are cleaned and not shown", async ({ page }) => {
		await expect(rowOf(page, "ghost")).toHaveCount(0);
	});

	test("loading the mappings does not write them back", async ({ page, tauri }) => {
		await page.waitForTimeout(300);
		expect(await tauri.calls("set_application_profiles")).toHaveLength(0);
	});

	test("profiles inside folders are grouped under an optgroup", async ({ page }) => {
		const select = page.getByTestId("application-add-profile");
		await expect(select.locator('optgroup[label="Work"] option')).toHaveText(["Dev", "Ops"]);
		await expect(select.locator('optgroup[label="Work"] option').first()).toHaveAttribute("value", "Work/Dev");
		await page.getByTestId("application-add-app").selectOption("code");
		await select.selectOption("Work/Ops");
		await expect(rowOf(page, "code").getByTestId("application-profile-select")).toHaveValue("Work/Ops");
	});

	test("removing a mapping keeps mappings of other devices", async ({ page, tauri }) => {
		await rowOf(page, "firefox").getByTestId("application-profile-select").selectOption({ label: "Remove application" });
		await expect.poll(() => lastSaved(tauri)).toEqual({ code: { "sd-OTHER": "Default" }, opendeck_default: { "sd-TEST": "Default", "sd-OTHER": "Default" } });
	});
});
