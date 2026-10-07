import { akp05Device, AKP05_ID, OTHER_ID, openApp, test, expect } from "./fixtures/tauriMock";

const both = () => ({ [AKP05_ID]: akp05Device(), [OTHER_ID]: akp05Device(OTHER_ID) });
const onlyMain = () => ({ [AKP05_ID]: akp05Device() });
const isOther = "(args) => args.device == 'sd-OTHER'";

test.describe("DeviceSelector", () => {
	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("registers each Tauri listener exactly once", async ({ tauri }) => {
		expect(await tauri.listenerCount("devices")).toBe(1);
		expect(await tauri.listenerCount("switch_profile")).toBe(1);
		expect(await tauri.listenerCount("rerender_images")).toBe(1);
		// One update_state listener per key/zone: 10 keys + 4 touch zones.
		expect(await tauri.listenerCount("update_state")).toBe(14);
	});

	test("devices event adds a second device to the selector", async ({ page, tauri }) => {
		await expect(page.getByTestId("device-selector").locator("option:not([disabled])")).toHaveCount(1);
		await tauri.emit("devices", both());
		await expect(page.getByTestId("device-selector").locator("option:not([disabled])")).toHaveCount(2);
		await expect(page.locator('[data-testid="device-view"]')).toHaveCount(2);
		// Only the selected device's view is visible.
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${AKP05_ID}"]`)).toBeVisible();
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${OTHER_ID}"]`)).toBeHidden();
		await page.getByTestId("device-selector").selectOption(OTHER_ID);
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${OTHER_ID}"]`)).toBeVisible();
	});

	test("a late profile response for a removed device is ignored", async ({ page, tauri }) => {
		await tauri.hold("get_selected_profile", isOther);
		await tauri.emit("devices", both());
		await expect.poll(async () => (await tauri.calls("get_selected_profile")).filter((c) => c.args.device == OTHER_ID).length).toBe(1);

		// The device is unplugged while its profile request is still in flight.
		await tauri.emit("devices", onlyMain());
		await expect(page.getByTestId("device-selector").locator("option:not([disabled])")).toHaveCount(1);

		await tauri.release("get_selected_profile");
		// Give the late response time to (incorrectly) land.
		await page.waitForTimeout(300);

		await expect(page.locator('[data-testid="device-view"]')).toHaveCount(1);
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${OTHER_ID}"]`)).toHaveCount(0);
		await expect(page.getByTestId("device-selector").locator(`option[value="${OTHER_ID}"]`)).toHaveCount(0);
		await expect(page.getByTestId("device-title")).toHaveText("Ajazz AKP05E_552A");
		await expect(page.getByTestId("device-view")).toBeVisible();
	});

	test("a device that is removed and re-added loads its profile again", async ({ page, tauri }) => {
		await tauri.hold("get_selected_profile", isOther);
		await tauri.emit("devices", both());
		await expect.poll(async () => (await tauri.calls("get_selected_profile")).filter((c) => c.args.device == OTHER_ID).length).toBe(1);
		await tauri.emit("devices", onlyMain());
		await tauri.emit("devices", both());
		// Re-registration triggers a fresh request instead of reusing the stale one.
		await expect.poll(async () => (await tauri.calls("get_selected_profile")).filter((c) => c.args.device == OTHER_ID).length).toBe(2);

		await tauri.release("get_selected_profile");
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${OTHER_ID}"]`)).toHaveCount(1);
		await page.getByTestId("device-selector").selectOption(OTHER_ID);
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${OTHER_ID}"]`)).toBeVisible();
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${OTHER_ID}"] [data-testid^="device-key-"][data-controller="Keypad"]`)).toHaveCount(10);
	});

	test("disconnecting the only device returns to the empty state", async ({ page, tauri }) => {
		await tauri.emit("devices", {});
		await expect(page.getByTestId("device-view")).toHaveCount(0);
		await expect(page.getByTestId("device-selector")).toHaveCount(0);
		await expect(page.getByTestId("device-title")).toHaveText("OpenDeck");
		await tauri.emit("devices", onlyMain());
		await expect(page.getByTestId("device-view")).toBeVisible();
		await expect(page.getByTestId("device-title")).toHaveText("Ajazz AKP05E_552A");
	});

	test("removing the selected device falls back to the remaining one", async ({ page, tauri }) => {
		await tauri.emit("devices", both());
		await page.getByTestId("device-selector").selectOption(OTHER_ID);
		await tauri.emit("devices", onlyMain());
		await expect(page.getByTestId("device-selector")).toHaveValue(AKP05_ID);
		await expect(page.locator(`[data-testid="device-view"][data-device-id="${AKP05_ID}"]`)).toBeVisible();
	});

	test("switch_profile event refreshes the profile of the active device", async ({ page, tauri }) => {
		await expect(page.getByTestId("profile-selector")).toHaveValue("Default");
		await tauri.mockState("(s) => { s.selected['sd-TEST'] = 'Gaming'; }");
		await tauri.emit("switch_profile", { device: AKP05_ID, profile: "Gaming" });
		await expect(page.getByTestId("profile-selector")).toHaveValue("Gaming");
	});

	test("switch_profile for an unknown device is ignored", async ({ page, tauri }) => {
		const before = (await tauri.calls("get_selected_profile")).length;
		await tauri.emit("switch_profile", { device: "sd-GHOST", profile: "Gaming" });
		await page.waitForTimeout(200);
		expect((await tauri.calls("get_selected_profile")).length).toBe(before);
	});

	test("reports the AKP05E_552A minimum window size through the window plugin", async ({ tauri }) => {
		// max(columns, encoders) * 132 + 392 = 1052
		await expect.poll(async () => (await tauri.calls("plugin:window|set_min_size")).length).toBeGreaterThan(0);
		const [call] = await tauri.calls("plugin:window|set_min_size");
		expect(JSON.stringify(call!.args)).toContain("1052");
	});
});
