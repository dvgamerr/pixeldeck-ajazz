import { defineConfig, devices } from "@playwright/test";

// The Tauri devUrl (src-tauri/tauri.conf.json) and the Vite default are both port 5173.
const PORT = 5173;

export default defineConfig({
	testDir: "./e2e",
	globalSetup: "./e2e/globalSetup.ts",
	// Not *.spec.ts: `bun test` would pick those files up.
	testMatch: "**/*.pw.ts",
	timeout: 45_000,
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	expect: { timeout: 15_000 },
	reporter: [["list"], ["html", { open: "never" }]],
	use: {
		baseURL: `http://localhost:${PORT}`,
		trace: "retain-on-failure",
		viewport: { width: 1440, height: 900 },
	},
	projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1440, height: 900 } } }],
	webServer: {
		command: "bun run dev",
		url: `http://localhost:${PORT}`,
		reuseExistingServer: true,
		timeout: 120_000,
	},
});
