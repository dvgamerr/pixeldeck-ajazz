import { expect, test } from "@playwright/test";

import { installTauriMock, uninstallTauriMock } from "./helpers/tauri";
import { freshImport } from "./helpers/fresh";

const g = globalThis as any;
let savedOpen: unknown;

test.beforeEach(() => {
	savedOpen = g.open;
});
test.afterEach(() => {
	g.open = savedOpen;
	uninstallTauriMock();
});

test.describe("shims (side-effect module)", () => {
	test("globalThis.open is routed to the open_url command and returns null", async () => {
		const tauri = installTauriMock();
		await freshImport("shims.ts");
		expect(g.open("https://example.com")).toBeNull();
		expect(tauri.calls).toEqual([{ cmd: "open_url", args: { url: "https://example.com" } }]);
	});

	test("open without a URL does not call the backend", async () => {
		const tauri = installTauriMock();
		await freshImport("shims.ts");
		expect(g.open()).toBeNull();
		expect(g.open("")).toBeNull();
		expect(tauri.calls).toHaveLength(0);
	});

	test("URL objects are forwarded as passed", async () => {
		const tauri = installTauriMock();
		await freshImport("shims.ts");
		const url = new URL("https://example.com/a");
		g.open(url);
		expect(tauri.calls[0].args.url).toBe(url);
	});
});
