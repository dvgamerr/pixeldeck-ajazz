import { expect, test } from "@playwright/test";
import { get } from "svelte/store";

import { installTauriMock, uninstallTauriMock } from "./helpers/tauri";
import { flushMicrotasks, freshImport } from "./helpers/fresh";

const g = globalThis as any;
let savedDocument: unknown;
let listeners: Record<string, (() => void)[]>;

test.beforeEach(() => {
	savedDocument = g.document;
	listeners = {};
	g.document = { addEventListener: (type: string, fn: () => void) => (listeners[type] ??= []).push(fn) };
});

test.afterEach(() => {
	g.document = savedDocument;
	uninstallTauriMock();
});

const ctx = { device: "d", profile: "P", controller: "Keypad", position: 1 };

test.describe("propertyInspector stores", () => {
	test("inspectedInstance starts null and notifies the backend of the (null -> null) switch on subscribe", async () => {
		const tauri = installTauriMock();
		const mod = await freshImport("propertyInspector.ts");
		await flushMicrotasks();
		expect(get(mod.inspectedInstance)).toBeNull();
		expect(tauri.calls).toEqual([{ cmd: "switch_property_inspector", args: { old: null, new: null } }]);
	});

	test("changing the inspected instance tells the backend the previous and new value", async () => {
		const tauri = installTauriMock();
		const mod = await freshImport("propertyInspector.ts");
		await flushMicrotasks();
		mod.inspectedInstance.set("ctx-a");
		await flushMicrotasks();
		mod.inspectedInstance.set("ctx-b");
		await flushMicrotasks();
		mod.inspectedInstance.set(null);
		await flushMicrotasks();
		expect(tauri.calls.slice(1).map((c) => c.args)).toEqual([
			{ old: null, new: "ctx-a" },
			{ old: "ctx-a", new: "ctx-b" },
			{ old: "ctx-b", new: null },
		]);
	});

	test("inspectedParentAction, openContextMenu and copiedContext start null and are writable", async () => {
		installTauriMock();
		const mod = await freshImport("propertyInspector.ts");
		for (const store of [mod.inspectedParentAction, mod.openContextMenu, mod.copiedContext]) expect(get(store)).toBeNull();
		mod.inspectedParentAction.set(ctx);
		mod.copiedContext.set(ctx);
		mod.openContextMenu.set({ context: ctx, x: 10, y: 20 });
		expect(get(mod.inspectedParentAction)).toEqual(ctx);
		expect(get(mod.copiedContext)).toEqual(ctx);
		expect(get(mod.openContextMenu)).toEqual({ context: ctx, x: 10, y: 20 });
	});

	test("a document click closes the open context menu", async () => {
		installTauriMock();
		const mod = await freshImport("propertyInspector.ts");
		expect(listeners.click).toHaveLength(1);
		mod.openContextMenu.set({ context: ctx, x: 1, y: 2 });
		listeners.click[0]();
		expect(get(mod.openContextMenu)).toBeNull();
	});
});
