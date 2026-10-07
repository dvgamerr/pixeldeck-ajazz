import { chromium, type FullConfig } from "@playwright/test";

/**
 * Vite compiles modules and optimises dependencies lazily. Without a warm-up, the first wave of
 * parallel workers all hit a cold dev server (and Vite may reload the page when it discovers new
 * dependencies), which makes the first tests flaky. Load the app once, up front, until it is idle.
 */
export default async function globalSetup(config: FullConfig) {
	const baseURL = config.projects[0]?.use.baseURL ?? "http://localhost:5173";
	const browser = await chromium.launch();
	try {
		const page = await browser.newPage();
		page.setDefaultTimeout(90_000);
		for (let attempt = 0; attempt < 2; attempt++) {
			await page.goto(baseURL, { waitUntil: "networkidle" }).catch(() => {});
			await page.waitForTimeout(1_500);
		}
	} finally {
		await browser.close();
	}
}
