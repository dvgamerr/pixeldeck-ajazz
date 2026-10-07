import type { Page } from "@playwright/test";

import { BOTH_ACTION, DIAL_ACTION, KEY_ACTION, makeAction, makeDataTransfer, openApp, test, expect } from "../fixtures/tauriMock";

const MULTI = { ...makeAction("opendeck.multiaction", "Multi Action", ["Keypad"], "opendeck"), icon: "opendeck/multi-action.png" };
const TOGGLE = { ...makeAction("opendeck.toggleaction", "Toggle Action", ["Keypad"], "opendeck"), icon: "opendeck/toggle-action.png" };
const NO_MULTI = { ...BOTH_ACTION, uuid: "test.plugin.nomulti", name: "Single Only", supported_in_multi_actions: false };

const openParent = async (page: Page, position = 1) => {
	await openApp(page);
	await page.getByTestId(`device-key-${position}`).getByTestId("key-canvas").click();
	await expect(page.getByTestId("parent-action-view")).toBeVisible();
};

const dropOnZone = async (page: Page, action: unknown, payload: Record<string, string> = { action: JSON.stringify(action) }) => {
	const transfer = await makeDataTransfer(page, payload);
	const zone = page.getByTestId("parent-action-dropzone");
	await zone.dispatchEvent("dragenter", { dataTransfer: transfer });
	await zone.dispatchEvent("dragover", { dataTransfer: transfer });
	await zone.dispatchEvent("drop", { dataTransfer: transfer });
};

test.describe("ParentActionView: multi action", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 1, action: MULTI, children: [KEY_ACTION, { ...BOTH_ACTION }] }] } });

	test("selecting a multi action key replaces the device view with the sequence editor", async ({ page }) => {
		await openParent(page);
		await expect(page.getByTestId("device-view")).toHaveCount(0);
		await expect(page.getByTestId("parent-action-view")).toHaveAttribute("data-parent-uuid", "opendeck.multiaction");
		await expect(page.getByTestId("parent-action-title")).toHaveText("Multi Action");
		await expect(page.getByTestId("parent-action-count")).toHaveText("2 actions");
		await expect(page.getByText("Each press runs Step 1 through the final step in order")).toBeVisible();
		await expect(page.getByText("Runs top to bottom")).toBeVisible();
	});

	test("the sidebar hides the device and profile selectors while editing a parent action", async ({ page }) => {
		await openApp(page);
		await expect(page.getByTestId("device-selector")).toBeVisible();
		await page.getByTestId("device-key-1").getByTestId("key-canvas").click();
		await expect(page.getByTestId("device-selector")).toHaveCount(0);
		await expect(page.getByTestId("profile-selector")).toHaveCount(0);
		await expect(page.getByTestId("action-list")).toBeVisible();
	});

	test("lists the children in order as numbered steps with action and plugin names", async ({ page }) => {
		await openParent(page);
		const children = page.getByTestId("parent-action-child");
		await expect(children).toHaveCount(2);
		await expect(children.nth(0).getByTestId("parent-action-child-label")).toHaveText("Step 1");
		await expect(children.nth(1).getByTestId("parent-action-child-label")).toHaveText("Step 2");
		await expect(children.nth(0)).toContainText("Key Only Action");
		await expect(children.nth(0)).toContainText("test.plugin");
		await expect(children.nth(1)).toContainText("Hybrid Action");
		await expect(children.nth(0)).toHaveAttribute("data-context", "sd-TEST.Default.Keypad.1.1");
		await expect(children.nth(1)).toHaveAttribute("data-context", "sd-TEST.Default.Keypad.1.2");
	});

	test("each child renders its own preview canvas", async ({ page }) => {
		await openParent(page);
		for (const child of await page.getByTestId("parent-action-child").all()) await expect(child.locator("canvas")).toHaveCount(1);
	});

	test("the back button returns to the device without touching the slot", async ({ page, tauri }) => {
		await openParent(page);
		await page.getByTestId("parent-action-back").click();
		await expect(page.getByTestId("parent-action-view")).toHaveCount(0);
		await expect(page.getByTestId("device-view")).toBeVisible();
		await expect(page.getByTestId("device-key-1")).toHaveAttribute("data-occupied", "true");
		expect(await tauri.calls("remove_instance")).toHaveLength(0);
	});

	test("Escape closes the view and clears the inspected instance", async ({ page, tauri }) => {
		await openParent(page);
		await page.getByTestId("parent-action-child").first().click();
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("parent-action-view")).toHaveCount(0);
		await expect(page.getByTestId("device-view")).toBeVisible();
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
		const last = (await tauri.calls("switch_property_inspector")).at(-1)!;
		expect(last.args.new).toBeNull();
	});

	test("clicking a child selects it and tells the backend which instance is inspected", async ({ page, tauri }) => {
		await openParent(page);
		const second = page.getByTestId("parent-action-child").nth(1);
		await second.click();
		await expect(second).toHaveAttribute("aria-pressed", "true");
		await expect(second).toContainText("Editing");
		await expect(page.getByTestId("parent-action-child").first()).toHaveAttribute("aria-pressed", "false");
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		const last = (await tauri.calls("switch_property_inspector")).at(-1)!;
		expect(last.args.new).toBe("sd-TEST.Default.Keypad.1.2");
	});

	test("selecting works from the keyboard with Enter and Space", async ({ page }) => {
		await openParent(page);
		const first = page.getByTestId("parent-action-child").first();
		const second = page.getByTestId("parent-action-child").nth(1);
		await first.focus();
		await page.keyboard.press("Enter");
		await expect(first).toHaveAttribute("aria-pressed", "true");
		await second.focus();
		await page.keyboard.press(" ");
		await expect(second).toHaveAttribute("aria-pressed", "true");
		await expect(first).toHaveAttribute("aria-pressed", "false");
	});

	test("clicking the empty background clears the selection", async ({ page }) => {
		await openParent(page);
		const first = page.getByTestId("parent-action-child").first();
		await first.click();
		await expect(first).toHaveAttribute("aria-pressed", "true");
		await page.getByText("Click an action to edit its settings.").click();
		await expect(first).toHaveAttribute("aria-pressed", "false");
	});

	test("removing a child calls remove_instance with its context and renumbers the rest", async ({ page, tauri }) => {
		await openParent(page);
		await page.getByTestId("parent-action-child").first().getByTestId("parent-action-child-remove").click();
		await expect(page.getByTestId("parent-action-child")).toHaveCount(1);
		await expect(page.getByTestId("parent-action-count")).toHaveText("1 action");
		await expect(page.getByTestId("parent-action-child").first()).toContainText("Hybrid Action");
		await expect(page.getByTestId("parent-action-child-label")).toHaveText("Step 1");
		const [call] = await tauri.calls("remove_instance");
		expect(call!.args).toEqual({ context: "sd-TEST.Default.Keypad.1.1" });
	});

	test("removing the inspected child closes the property inspector", async ({ page }) => {
		await openParent(page);
		const first = page.getByTestId("parent-action-child").first();
		await first.click();
		await expect(page.getByTestId("property-inspector-panel")).toBeVisible();
		await first.getByTestId("parent-action-child-remove").click();
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
	});

	test("removing a child does not select it", async ({ page, tauri }) => {
		await openParent(page);
		const before = (await tauri.calls("switch_property_inspector")).length;
		await page.getByTestId("parent-action-child").nth(1).getByTestId("parent-action-child-remove").click();
		await expect(page.getByTestId("parent-action-child")).toHaveCount(1);
		const calls = await tauri.calls("switch_property_inspector");
		expect(calls.slice(before).filter((call) => call.args.new)).toEqual([]);
	});

	test("dropping a library action appends a step via create_instance on the parent context", async ({ page, tauri }) => {
		await openParent(page);
		await dropOnZone(page, DIAL_ACTION);
		// A Dial-only action is still accepted here: the parent view does not filter by controller.
		await expect(page.getByTestId("parent-action-child")).toHaveCount(3);

		await dropOnZone(page, KEY_ACTION);
		await expect(page.getByTestId("parent-action-child")).toHaveCount(4);
		await expect(page.getByTestId("parent-action-count")).toHaveText("4 actions");
		await expect(page.getByTestId("parent-action-child").last()).toContainText("Key Only Action");
		await expect(page.getByTestId("parent-action-child-label").last()).toHaveText("Step 4");
		const calls = await tauri.calls("create_instance");
		expect(calls.at(-1)!.args.context).toEqual({ device: "sd-TEST", profile: "Default", controller: "Keypad", position: 1 });
		expect(calls.at(-1)!.args.action.uuid).toBe("test.plugin.key");
	});

	test("a freshly dropped step is selected for editing", async ({ page, tauri }) => {
		await openParent(page);
		await dropOnZone(page, KEY_ACTION);
		const added = page.getByTestId("parent-action-child").last();
		await expect(added).toHaveAttribute("aria-pressed", "true");
		await expect.poll(async () => (await tauri.calls("switch_property_inspector")).at(-1)?.args.new).toBe("sd-TEST.Default.Keypad.1.3");
	});

	test("actions that are not supported in multi actions are rejected", async ({ page, tauri }) => {
		await openParent(page);
		await dropOnZone(page, NO_MULTI);
		await expect(page.getByTestId("parent-action-child")).toHaveCount(2);
		expect(await tauri.calls("create_instance")).toHaveLength(0);
	});

	test("drops without an action payload are ignored", async ({ page, tauri }) => {
		await openParent(page);
		await dropOnZone(page, null, { controller: "Keypad", position: "3" });
		await expect(page.getByTestId("parent-action-child")).toHaveCount(2);
		expect(await tauri.calls("create_instance")).toHaveLength(0);
	});

	test("the drop zone highlights while dragging over it and the dragover default is prevented", async ({ page }) => {
		await openParent(page);
		const zone = page.getByTestId("parent-action-dropzone");
		await expect(zone).not.toHaveClass(/border-primary/);
		const transfer = await makeDataTransfer(page, { action: "{}" });
		await zone.dispatchEvent("dragenter", { dataTransfer: transfer });
		await expect(zone).toHaveClass(/border-primary/);
		await zone.dispatchEvent("dragleave", { dataTransfer: transfer });
		await expect(zone).not.toHaveClass(/border-primary/);
		const prevented = await zone.evaluate((node) => {
			const event = new DragEvent("dragover", { bubbles: true, cancelable: true });
			node.dispatchEvent(event);
			return event.defaultPrevented;
		});
		expect(prevented).toBe(true);
	});

	test("the highlight is cleared after a drop", async ({ page }) => {
		await openParent(page);
		await dropOnZone(page, KEY_ACTION);
		await expect(page.getByTestId("parent-action-dropzone")).not.toHaveClass(/border-primary/);
	});

	test("the prompt reads next step once children exist", async ({ page }) => {
		await openParent(page);
		await expect(page.getByTestId("parent-action-dropzone")).toContainText("Add next step");
	});

	test("child previews are inert: right click opens no context menu", async ({ page }) => {
		await openParent(page);
		await page.getByTestId("parent-action-child").first().locator("canvas").click({ button: "right" });
		await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
	});
});

test.describe("ParentActionView: toggle action", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 4, action: TOGGLE, children: [KEY_ACTION, BOTH_ACTION] }] } });

	test("uses toggle wording and state labels", async ({ page }) => {
		await openParent(page, 4);
		await expect(page.getByTestId("parent-action-view")).toHaveAttribute("data-parent-uuid", "opendeck.toggleaction");
		await expect(page.getByTestId("parent-action-title")).toHaveText("Toggle Action");
		await expect(page.getByTestId("parent-action-child-label")).toHaveText(["State 1", "State 2"]);
		await expect(page.getByText("Repeats after the last state")).toBeVisible();
		await expect(page.getByTestId("parent-action-dropzone")).toContainText("Add another state");
	});

	test("rejects nesting multi and toggle actions", async ({ page, tauri }) => {
		await openParent(page, 4);
		await dropOnZone(page, MULTI);
		await dropOnZone(page, TOGGLE);
		await expect(page.getByTestId("parent-action-child")).toHaveCount(2);
		expect(await tauri.calls("create_instance")).toHaveLength(0);
	});

	test("accepts a normal action, even one that is not multi-action capable", async ({ page, tauri }) => {
		await openParent(page, 4);
		await dropOnZone(page, NO_MULTI);
		await expect(page.getByTestId("parent-action-child")).toHaveCount(3);
		expect(await tauri.calls("create_instance")).toHaveLength(1);
	});
});

test.describe("ParentActionView: empty and singular states", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 1, action: MULTI, children: [] }] } });

	test("an empty multi action prompts for the first action", async ({ page }) => {
		await openParent(page);
		await expect(page.getByTestId("parent-action-count")).toHaveText("0 actions");
		await expect(page.getByTestId("parent-action-child")).toHaveCount(0);
		await expect(page.getByTestId("parent-action-dropzone")).toContainText("Add your first action");
	});

	test("dropping the first action makes the count singular", async ({ page }) => {
		await openParent(page);
		await dropOnZone(page, KEY_ACTION);
		await expect(page.getByTestId("parent-action-count")).toHaveText("1 action");
	});
});
