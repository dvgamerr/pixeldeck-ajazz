import { expect, test } from "../fixtures/tauriMock";

test.describe("NoDevicesDetected", () => {
	test.use({ options: { devices: {} } });

	test("is shown instead of the device view when no device is connected", async ({ page }) => {
		await page.goto("/");
		await expect(page.getByTestId("no-devices")).toBeVisible();
		await expect(page.getByTestId("device-view")).toHaveCount(0);
		await expect(page.getByTestId("no-devices")).toContainText("No devices detected");
		await expect(page.getByTestId("device-title")).toHaveText("PixelDeck");
		await expect(page.getByTestId("device-connected-badge")).toHaveCount(0);
		// No device means no device selector and no property inspector panel.
		await expect(page.getByTestId("property-inspector-panel")).toHaveCount(0);
	});

	test("shows the udev hint when the build info reports Linux", async ({ page }) => {
		await page.goto("/");
		await expect(page.getByTestId("no-devices-udev-hint")).toContainText("udev");
	});

	test("hides the udev hint on other platforms", async ({ page, tauri }) => {
		await page.addInitScript(() => {
			(window as any).__mock.state.buildInfo = "windows x86_64 e2e-mock";
		});
		await page.goto("/");
		await expect(page.getByTestId("no-devices")).toBeVisible();
		await expect.poll(async () => (await tauri.calls("get_build_info")).length).toBeGreaterThan(0);
		await expect(page.getByTestId("no-devices-udev-hint")).toHaveCount(0);
	});

	test("the restart button asks the backend to restart the application", async ({ page, tauri }) => {
		await page.goto("/");
		const button = page.getByTestId("no-devices-restart");
		await expect(button).toContainText("Restart");
		await button.click();
		await expect.poll(async () => (await tauri.calls("restart")).length).toBe(1);
		expect((await tauri.calls("restart"))[0]!.args).toEqual({});
	});

	test("the workspace appears when a device connects", async ({ page, tauri }) => {
		await page.goto("/");
		await expect(page.getByTestId("no-devices")).toBeVisible();
		await tauri.mockState(`(s) => { s.devices["sd-LATE"] = { id: "sd-LATE", name: "Ajazz AKP05E_552A", rows: 2, columns: 5, encoders: 4, type: 7 }; s.profileNames["sd-LATE"] = ["Default"]; }`);
		await tauri.emit("devices", { "sd-LATE": { id: "sd-LATE", name: "Ajazz AKP05E_552A", rows: 2, columns: 5, encoders: 4, type: 7 } });
		await expect(page.getByTestId("device-view")).toBeVisible();
		await expect(page.getByTestId("no-devices")).toHaveCount(0);
	});
});
