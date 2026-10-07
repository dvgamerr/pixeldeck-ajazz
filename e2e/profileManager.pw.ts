import type { Page } from "@playwright/test";

import { openApp, test, expect } from "./fixtures/tauriMock";

const openManager = async (page: Page) => {
	await page.getByTestId("profile-selector").selectOption("opendeck_edit_profiles");
	await expect(page.getByTestId("profile-manager")).toBeVisible();
};

const row = (page: Page, profile: string) => page.locator(`[data-testid="profile-item"][data-profile="${profile}"]`);

test.describe("ProfileManager", () => {
	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("lists the profiles of the device in the selector", async ({ page }) => {
		const selector = page.getByTestId("profile-selector");
		await expect(selector).toHaveValue("Default");
		await expect(selector.locator("option")).toHaveText(["Default", "Gaming", "Edit..."]);
	});

	test("selecting a profile calls set_selected_profile", async ({ page, tauri }) => {
		await page.getByTestId("profile-selector").selectOption("Gaming");
		await expect(page.getByTestId("profile-selector")).toHaveValue("Gaming");
		await expect.poll(async () => (await tauri.calls("set_selected_profile")).length).toBe(1);
		expect((await tauri.calls("set_selected_profile"))[0]!.args).toEqual({ device: "sd-TEST", id: "Gaming" });
	});

	test("the Edit... option opens the manager and keeps the current selection", async ({ page }) => {
		await openManager(page);
		await expect(page.getByTestId("profile-selector")).toHaveValue("Default");
		await expect(page.getByTestId("profile-item")).toHaveCount(2);
		await expect(row(page, "Default").getByTestId("profile-radio")).toBeChecked();
	});

	test("closes with the header close button and Escape", async ({ page }) => {
		await openManager(page);
		await page.getByTestId("profile-manager-header-close").click();
		await expect(page.getByTestId("profile-manager")).toHaveCount(0);
		await openManager(page);
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("profile-manager")).toHaveCount(0);
	});

	test("creates a profile and selects it", async ({ page, tauri }) => {
		await openManager(page);
		await page.getByTestId("profile-name-input").fill("Streaming");
		await page.getByTestId("profile-create").click();

		await expect(page.getByTestId("profile-manager")).toHaveCount(0);
		await expect(page.getByTestId("profile-selector")).toHaveValue("Streaming");
		const ids = (await tauri.calls("set_selected_profile")).map((c) => c.args.id);
		expect(ids).toContain("Streaming");
		expect(await tauri.mockState("(s) => s.profileNames['sd-TEST']")).toContain("Streaming");
	});

	test("creates a profile inside a folder", async ({ page }) => {
		await openManager(page);
		await page.getByTestId("profile-name-input").fill("Work/Docs");
		await page.getByTestId("profile-create").click();
		await expect(page.getByTestId("profile-selector")).toHaveValue("Work/Docs");
	});

	test("rejects invalid or empty profile names without calling the backend", async ({ page, tauri }) => {
		await openManager(page);
		await page.getByTestId("profile-name-input").fill("bad*name!");
		await page.getByTestId("profile-create").click();
		await expect(page.getByTestId("profile-manager")).toBeVisible();
		await page.getByTestId("profile-name-input").fill("");
		await page.getByTestId("profile-create").click();
		await expect(page.getByTestId("profile-manager")).toBeVisible();
		expect(await tauri.calls("set_selected_profile")).toHaveLength(0);
	});

	test("renames a profile", async ({ page, tauri }) => {
		await openManager(page);
		await row(page, "Gaming").getByTestId("profile-rename").click();
		const input = page.getByTestId("profile-rename-input");
		await expect(input).toHaveValue("Gaming");
		await input.fill("Racing");
		await page.getByTestId("profile-rename-save").click();

		await expect(row(page, "Racing")).toBeVisible();
		await expect(row(page, "Gaming")).toHaveCount(0);
		const [call] = await tauri.calls("rename_profile");
		expect(call!.args).toEqual({ device: "sd-TEST", profile: "Gaming", newId: "Racing" });
	});

	test("rename can be cancelled and an unchanged name is a no-op", async ({ page, tauri }) => {
		await openManager(page);
		await row(page, "Gaming").getByTestId("profile-rename").click();
		await page.getByTestId("profile-rename-cancel").click();
		await expect(page.getByTestId("profile-rename-input")).toHaveCount(0);
		await row(page, "Gaming").getByTestId("profile-rename").click();
		await page.getByTestId("profile-rename-save").click();
		await expect(page.getByTestId("profile-rename-input")).toHaveCount(0);
		expect(await tauri.calls("rename_profile")).toHaveLength(0);
	});

	test("rename validates the name locally and surfaces backend errors", async ({ page, tauri }) => {
		await openManager(page);
		await row(page, "Gaming").getByTestId("profile-rename").click();
		await page.getByTestId("profile-rename-input").fill("no/slash/twice");
		await page.getByTestId("profile-rename-save").click();
		await expect(page.getByTestId("profile-manager-error")).toContainText("Profile names may contain");
		expect(await tauri.calls("rename_profile")).toHaveLength(0);

		await tauri.setRenameError("profile exists");
		await page.getByTestId("profile-rename-input").fill("Valid");
		await page.getByTestId("profile-rename-save").click();
		await expect(page.getByTestId("profile-manager-error")).toContainText("Unable to rename profile: profile exists");
	});

	test("deletes a profile that is not selected", async ({ page, tauri }) => {
		await openManager(page);
		// The selected profile cannot be deleted.
		await expect(row(page, "Default").getByTestId("profile-delete")).toHaveCount(0);
		await row(page, "Gaming").getByTestId("profile-delete").click();

		await expect(row(page, "Gaming")).toHaveCount(0);
		const [call] = await tauri.calls("delete_profile");
		expect(call!.args).toEqual({ device: "sd-TEST", profile: "Gaming" });
		await expect(page.getByTestId("profile-selector").locator('option[value="Gaming"]')).toHaveCount(0);
	});

	test("opens the application mapping popup with the detected applications", async ({ page }) => {
		await openManager(page);
		await page.getByTestId("profile-application-mapping").click();
		await expect(page.getByTestId("application-add-app")).toBeVisible();
		await expect(page.getByTestId("application-add-app").locator("option")).toContainText(["firefox", "code"]);
	});
});
