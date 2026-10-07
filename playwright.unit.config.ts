import { defineConfig } from "@playwright/test";

// Pure-logic unit tests for src/lib. No browser, no dev server.
export default defineConfig({
	testDir: "./unit",
	// Not *.spec.ts / *.test.*: `bun test` would pick those files up.
	testMatch: "**/*.unit.pw.ts",
	timeout: 15_000,
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	reporter: [["list"]],
	projects: [{ name: "unit" }],
});
