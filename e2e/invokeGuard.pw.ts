import { openApp, test, expect } from "./fixtures/tauriMock";

test.describe("unmatched invoke guard", () => {
	test("a full app load only invokes commands the mock knows about", async ({ page, tauri }) => {
		await openApp(page);
		await expect(page.getByTestId("action-item").first()).toBeVisible();
		expect(await tauri.unmatched()).toEqual([]);

		const commands = new Set((await tauri.calls()).map((call) => call.cmd));
		for (const expected of ["get_port_base", "get_settings", "get_devices", "get_selected_profile", "get_profiles", "get_categories", "list_plugins", "get_applications"]) {
			expect(commands, `expected startup to invoke ${expected}`).toContain(expected);
		}
	});

	test("an unknown command is recorded as unmatched (the guard fires)", async ({ page, tauri }) => {
		await openApp(page);
		const result = await page.evaluate(() => (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<unknown> } }).__TAURI_INTERNALS__.invoke("definitely_not_a_command"));
		expect(result).toBeNull();
		expect(await tauri.unmatched()).toEqual([{ cmd: "definitely_not_a_command", args: {} }]);
		// Acknowledge it so the auto fixture teardown does not fail this deliberate case.
		await tauri.clearUnmatched();
	});

	test("the page has no console errors or uncaught exceptions while loading", async ({ page }) => {
		const problems: string[] = [];
		page.on("pageerror", (error) => problems.push(`pageerror: ${error.message}`));
		page.on("console", (message) => {
			if (message.type() == "error" && !/Failed to load resource/.test(message.text())) problems.push(`console: ${message.text()}`);
		});
		await openApp(page);
		await expect(page.getByTestId("action-item").first()).toBeVisible();
		expect(problems).toEqual([]);
	});
});
