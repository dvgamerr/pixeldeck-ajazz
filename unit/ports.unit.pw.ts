import { expect, test } from "@playwright/test";

import { installTauriMock, uninstallTauriMock } from "./helpers/tauri";
import { freshImport } from "./helpers/fresh";

test.afterEach(() => uninstallTauriMock());

test.describe("ports", () => {
	test("defaults to base port 57116 before initialisation", async () => {
		installTauriMock();
		const ports = await freshImport("ports.ts");
		expect(ports.getWebSocketPort()).toBe(57116);
		expect(ports.getWebserverUrl()).toBe("http://127.0.0.1:57118/");
		expect(ports.getWebserverUrl("icons/a.png")).toBe("http://127.0.0.1:57118/icons/a.png");
	});

	test("initPortBase adopts the backend port; webserver is base + 2", async () => {
		const tauri = installTauriMock(() => 60000);
		const ports = await freshImport("ports.ts");
		await ports.initPortBase();
		expect(tauri.calls).toEqual([{ cmd: "get_port_base", args: {} }]);
		expect(ports.getWebSocketPort()).toBe(60000);
		expect(ports.getWebserverUrl("x")).toBe("http://127.0.0.1:60002/x");
	});

	test("concurrent and repeated calls share one invoke", async () => {
		const tauri = installTauriMock(() => 61000);
		const ports = await freshImport("ports.ts");
		const first = ports.initPortBase();
		const second = ports.initPortBase();
		await Promise.all([first, second]);
		await ports.initPortBase();
		expect(tauri.calls).toHaveLength(1);
	});

	test("a failed init rejects, keeps the default port and can be retried", async () => {
		let fail = true;
		const tauri = installTauriMock(() => {
			if (fail) throw new Error("not ready");
			return 62000;
		});
		const ports = await freshImport("ports.ts");
		await expect(ports.initPortBase()).rejects.toThrow("not ready");
		expect(ports.getWebSocketPort()).toBe(57116);
		fail = false;
		await ports.initPortBase();
		expect(tauri.calls).toHaveLength(2);
		expect(ports.getWebSocketPort()).toBe(62000);
	});
});
