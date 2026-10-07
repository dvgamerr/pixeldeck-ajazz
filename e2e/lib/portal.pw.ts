import { expect, test } from "../fixtures/tauriMock";
import { openForModules } from "./support";

const MOD = "/src/lib/portal.ts";

test.beforeEach(async ({ page }) => {
	await openForModules(page);
});

test.describe("portalToBody", () => {
	test("moves the node to the end of document.body", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { portalToBody } = await import(mod);
			const host = document.createElement("section");
			document.body.appendChild(host);
			const node = document.createElement("div");
			host.appendChild(node);
			portalToBody(node);
			return { parentIsBody: node.parentElement === document.body, last: document.body.lastElementChild === node, hostChildren: host.children.length };
		}, MOD);
		expect(result).toEqual({ parentIsBody: true, last: true, hostChildren: 0 });
	});

	test("destroy removes the node from the document", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { portalToBody } = await import(mod);
			const node = document.createElement("div");
			const action = portalToBody(node);
			const attached = node.isConnected;
			action.destroy();
			return { attached, afterDestroy: node.isConnected };
		}, MOD);
		expect(result).toEqual({ attached: true, afterDestroy: false });
	});

	test("escapes an ancestor that clips its overflow", async ({ page }) => {
		// This is why the key context menu is portalled: position: fixed inside a transformed/overflow ancestor would be clipped.
		const result = await page.evaluate(async (mod) => {
			const { portalToBody } = await import(mod);
			const clip = document.createElement("div");
			clip.style.cssText = "position:absolute;left:0;top:0;width:10px;height:10px;overflow:hidden;transform:translateZ(0)";
			const node = document.createElement("div");
			node.style.cssText = "position:fixed;left:200px;top:200px;width:50px;height:50px;background:red";
			clip.appendChild(node);
			document.body.appendChild(clip);
			portalToBody(node);
			const rect = node.getBoundingClientRect();
			return { x: rect.x, y: rect.y, width: rect.width };
		}, MOD);
		expect(result).toEqual({ x: 200, y: 200, width: 50 });
	});
});

test.describe("portalToPreviewDock", () => {
	test("moves the node into the .device-workspace element", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { portalToPreviewDock } = await import(mod);
			const node = document.createElement("div");
			document.body.appendChild(node);
			portalToPreviewDock(node);
			const dock = document.querySelector(".device-workspace");
			return { inDock: node.parentElement === dock, testId: dock?.getAttribute("data-testid") };
		}, MOD);
		expect(result).toEqual({ inDock: true, testId: "device-workspace" });
	});

	test("destroy removes the node again", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { portalToPreviewDock } = await import(mod);
			const node = document.createElement("div");
			const action = portalToPreviewDock(node);
			const attached = node.isConnected;
			action.destroy();
			return { attached, afterDestroy: node.isConnected };
		}, MOD);
		expect(result).toEqual({ attached: true, afterDestroy: false });
	});

	test("without a dock the node is left where it was and destroy still removes it", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { portalToPreviewDock } = await import(mod);
			document.querySelector(".device-workspace")!.classList.remove("device-workspace");
			const host = document.createElement("div");
			document.body.appendChild(host);
			const node = document.createElement("div");
			host.appendChild(node);
			const action = portalToPreviewDock(node);
			const stayed = node.parentElement === host;
			action.destroy();
			return { stayed, afterDestroy: node.isConnected };
		}, MOD);
		expect(result).toEqual({ stayed: true, afterDestroy: false });
	});
});
