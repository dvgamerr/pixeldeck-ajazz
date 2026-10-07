import { ACTION_MIME, BOTH_ACTION, DIAL_ACTION, dropActionOn, KEY_ACTION, makeDataTransfer, openApp, test, expect } from "./fixtures/tauriMock";

test.describe("drag and drop onto the device", () => {
	test.beforeEach(async ({ page }) => {
		await openApp(page);
	});

	test("a real pointer drag from the action library creates an instance on the key", async ({ page, tauri }) => {
		const item = page.locator('[data-testid="action-item"][data-action-uuid="test.plugin.key"]');
		await item.dragTo(page.getByTestId("device-key-3").getByTestId("key-canvas"));

		await expect(page.getByTestId("device-key-3")).toHaveAttribute("data-occupied", "true");
		const [call] = await tauri.calls("create_instance");
		expect(call!.args.context).toEqual({ device: "sd-TEST", profile: "Default", controller: "Keypad", position: 3 });
		expect(call!.args.action.uuid).toBe(KEY_ACTION.uuid);
	});

	test("dropping a Keypad action on a key records create_instance with the key context", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-6", KEY_ACTION);
		await expect(page.getByTestId("device-key-6")).toHaveAttribute("data-occupied", "true");
		const calls = await tauri.calls("create_instance");
		expect(calls).toHaveLength(1);
		expect(calls[0]!.args.context).toEqual({ device: "sd-TEST", profile: "Default", controller: "Keypad", position: 6 });
		expect(await tauri.calls("remove_instance")).toHaveLength(0);
	});

	test("the legacy 'action' payload alone is still accepted", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-1", KEY_ACTION, ["action"]);
		await expect(page.getByTestId("device-key-1")).toHaveAttribute("data-occupied", "true");
		expect(await tauri.calls("create_instance")).toHaveLength(1);
	});

	test("a Dial action dropped on a touch zone creates an Encoder instance", async ({ page, tauri }) => {
		await dropActionOn(page, "device-touch-2", DIAL_ACTION);
		await expect(page.getByTestId("device-touch-2")).toHaveAttribute("data-occupied", "true");
		const [call] = await tauri.calls("create_instance");
		expect(call!.args.context).toEqual({ device: "sd-TEST", profile: "Default", controller: "Encoder", position: 2 });
	});

	test("an action supporting both controllers can be dropped on keys and zones", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-0", BOTH_ACTION);
		await dropActionOn(page, "device-touch-0", BOTH_ACTION);
		const controllers = (await tauri.calls("create_instance")).map((call) => call.args.context.controller);
		expect(controllers).toEqual(["Keypad", "Encoder"]);
	});

	test("rejects a Dial-only action dropped on a key", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-5", DIAL_ACTION);
		await expect(page.getByTestId("device-key-5")).toHaveAttribute("data-occupied", "false");
		expect(await tauri.calls("create_instance")).toHaveLength(0);
		expect(await tauri.calls("remove_instance")).toHaveLength(0);
	});

	test("rejects a Key-only action dropped on a touch zone", async ({ page, tauri }) => {
		await dropActionOn(page, "device-touch-1", KEY_ACTION);
		await expect(page.getByTestId("device-touch-1")).toHaveAttribute("data-occupied", "false");
		expect(await tauri.calls("create_instance")).toHaveLength(0);
	});

	test("a rejected drop on an occupied slot keeps the existing instance", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-4", KEY_ACTION);
		await expect(page.getByTestId("device-key-4")).toHaveAttribute("data-occupied", "true");
		await dropActionOn(page, "device-key-4", DIAL_ACTION);
		await expect(page.getByTestId("device-key-4")).toHaveAttribute("data-occupied", "true");
		expect(await tauri.calls("remove_instance")).toHaveLength(0);
		expect(await tauri.calls("create_instance")).toHaveLength(1);
	});

	test("replacing an occupied slot removes the old instance before creating the new one", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-4", KEY_ACTION);
		await expect(page.getByTestId("device-key-4")).toHaveAttribute("data-occupied", "true");
		await dropActionOn(page, "device-key-4", BOTH_ACTION);
		await expect.poll(async () => (await tauri.calls("create_instance")).length).toBe(2);

		const sequence = (await tauri.calls()).map((call) => call.cmd).filter((cmd) => cmd == "create_instance" || cmd == "remove_instance");
		expect(sequence).toEqual(["create_instance", "remove_instance", "create_instance"]);
		const [remove] = await tauri.calls("remove_instance");
		expect(remove!.args.context).toBe("sd-TEST.Default.Keypad.4.0");
	});

	test("drop and dragover handlers call preventDefault", async ({ page }) => {
		const transfer = await makeDataTransfer(page, { [ACTION_MIME]: JSON.stringify(KEY_ACTION) });
		const canvas = page.getByTestId("device-key-8").getByTestId("key-canvas");
		const result = await canvas.evaluate((node, dt) => {
			const dragover = new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt });
			node.dispatchEvent(dragover);
			const drop = new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt });
			let reachedWindow = false;
			const listener = () => (reachedWindow = true);
			// stopPropagation in the key handler keeps the drop from bubbling to document listeners.
			document.addEventListener("drop", listener);
			node.dispatchEvent(drop);
			document.removeEventListener("drop", listener);
			return { dragover: dragover.defaultPrevented, drop: drop.defaultPrevented, reachedWindow };
		}, transfer);
		expect(result).toEqual({ dragover: true, drop: true, reachedWindow: false });
	});

	test("an empty slot is not draggable but an occupied one is", async ({ page }) => {
		await expect(page.getByTestId("device-key-0").getByTestId("key-canvas")).toHaveAttribute("draggable", "false");
		await dropActionOn(page, "device-key-0", KEY_ACTION);
		await expect(page.getByTestId("device-key-0").getByTestId("key-canvas")).toHaveAttribute("draggable", "true");
	});

	test("moving a configured key to another key calls move_instance without retain", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-0", KEY_ACTION);
		await expect(page.getByTestId("device-key-0")).toHaveAttribute("data-occupied", "true");

		const transfer = await makeDataTransfer(page, {});
		await page.getByTestId("device-key-0").getByTestId("key-canvas").dispatchEvent("dragstart", { dataTransfer: transfer });
		const target = page.getByTestId("device-key-9").getByTestId("key-canvas");
		await target.dispatchEvent("dragover", { dataTransfer: transfer });
		await target.dispatchEvent("drop", { dataTransfer: transfer });

		await expect(page.getByTestId("device-key-9")).toHaveAttribute("data-occupied", "true");
		await expect(page.getByTestId("device-key-0")).toHaveAttribute("data-occupied", "false");
		const [call] = await tauri.calls("move_instance");
		expect(call!.args).toMatchObject({
			source: { controller: "Keypad", position: 0 },
			destination: { controller: "Keypad", position: 9 },
			retain: false,
		});
	});

	test("dropping a key onto itself does nothing", async ({ page, tauri }) => {
		await dropActionOn(page, "device-key-0", KEY_ACTION);
		const transfer = await makeDataTransfer(page, {});
		const canvas = page.getByTestId("device-key-0").getByTestId("key-canvas");
		await canvas.dispatchEvent("dragstart", { dataTransfer: transfer });
		await canvas.dispatchEvent("drop", { dataTransfer: transfer });
		expect(await tauri.calls("move_instance")).toHaveLength(0);
	});
});
