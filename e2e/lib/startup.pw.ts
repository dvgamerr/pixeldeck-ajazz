import { expect, test } from "../fixtures/tauriMock";
import { openForModules } from "./support";

test.describe("startup loader (real DOM)", () => {
	test("the boot loader from app.html is removed once the services are ready", async ({ page }) => {
		await openForModules(page);
		await expect(page.locator("#startup-loader")).toHaveCount(0);
	});

	test("the boot loader is shown with the first task message while services are pending", async ({ page, tauri }) => {
		await page.addInitScript(() => {
			// Hold get_port_base so the loader stays visible; released by the test.
			const interval = setInterval(() => {
				const mock = (window as any).__mock;
				if (!mock) return;
				clearInterval(interval);
				mock.hold("get_port_base");
			}, 1);
		});
		await page.goto("/");
		await expect(page.locator("#startup-loader")).toBeVisible();
		await expect(page.locator("#startup-status-text")).toHaveText("Preparing application services…");
		await tauri.release("get_port_base");
		await expect(page.locator("#startup-loader")).toHaveCount(0);
	});

	test("status text walks through the pending tasks, then the loader fades out and is removed", async ({ page }) => {
		await page.addInitScript(() => {
			const w = window as any;
			w.__startupLog = { texts: [] as string[], leaving: false, ariaHidden: null as string | null };
			new MutationObserver(() => {
				const status = document.getElementById("startup-status-text");
				if (status && w.__startupLog.texts.at(-1) !== status.textContent) w.__startupLog.texts.push(status.textContent);
				const loader = document.getElementById("startup-loader");
				if (loader?.classList.contains("startup-loader--leaving")) {
					w.__startupLog.leaving = true;
					w.__startupLog.ariaHidden = loader.getAttribute("aria-hidden");
				}
			}).observe(document, { subtree: true, childList: true, characterData: true, attributes: true });
		});
		await page.goto("/");
		await expect(page.locator("#startup-loader")).toHaveCount(0);
		const log = await page.evaluate(() => (window as any).__startupLog);
		expect(log.texts[0]).toBe("Preparing application services…");
		expect(log.texts.at(-1)).toBe("Finalizing workspace…");
		expect(log.leaving).toBe(true);
		expect(log.ariaHidden).toBe("true");
	});
});
