import type { Page } from "@playwright/test";

import { openApp, test, expect, type TauriMock } from "../fixtures/tauriMock";

type Asset = { name: string; browser_download_url: string; download_count?: number };

const RAW = "https://raw.githubusercontent.com";
const README = [
	"# Alpha Plugin",
	"",
	"Some **bold** text and a [relative guide](docs/guide.md).",
	"",
	"[External](https://example.com/page)",
	"",
	"<script>window.__pwned = true</script>",
	'<img src="x" onerror="window.__pwned = true">',
].join("\n");

/** Stub every GitHub endpoint the store touches. */
const stubGithub = async (page: Page, options: { readmes?: Record<string, string>; releases?: Record<string, Asset[][] | "fail" | "abort"> } = {}) => {
	await page.context().route("https://github.com/**", (route) => route.fulfill({ status: 200, contentType: "text/html", body: "<title>github</title>" }));
	await page.route(`${RAW}/**`, (route) => {
		const body = options.readmes?.[route.request().url().slice(RAW.length)];
		return body === undefined ? route.fulfill({ status: 404, body: "Not Found" }) : route.fulfill({ status: 200, contentType: "text/plain", body });
	});
	await page.route("https://api.github.com/repos/**", (route) => {
		const repo = new URL(route.request().url()).pathname.split("/").slice(2, 4).join("/");
		const releases = options.releases?.[repo];
		if (releases === "abort") return route.abort();
		if (releases === "fail") return route.fulfill({ status: 500, body: "boom" });
		return route.fulfill({ json: (releases ?? []).map((assets) => ({ assets })) });
	});
};

const openStore = async (page: Page) => {
	await openApp(page);
	await page.getByTestId("plugins-open").click();
	await expect(page.getByTestId("plugin-manager")).toBeVisible();
	await expect(page.getByTestId("plugins-store")).toBeVisible();
};

const storeItem = (page: Page, name: string) => page.getByTestId("plugins-store").locator(`[data-testid="plugin-item"][data-plugin-name="${name}"]`);
const archiveItem = (page: Page, name: string) => page.getByTestId("plugins-archive").locator(`[data-testid="plugin-item"][data-plugin-name="${name}"]`);
const openDetails = async (page: Page, name: string) => {
	await storeItem(page, name).getByTestId("plugin-action").click();
	await expect(page.getByTestId("plugin-details")).toBeVisible();
};

// plugin-dialog implements ask() as a "message" command with YesNo buttons.
const dialogCalls = async (tauri: TauriMock, kind: "ask" | "message") =>
	(await tauri.calls("plugin:dialog|message")).map((call) => call.args).filter((args) => (args.buttons === "YesNo") == (kind == "ask"));
const decline = (tauri: TauriMock) => tauri.mockState("(s) => { s.dialogAsk = false; }");

const ZIP = (name: string, count = 0): Asset => ({ name, browser_download_url: `https://downloads.invalid/${name}`, download_count: count });

test.describe("PluginDetails", () => {
	test.beforeEach(async ({ page }) => {
		await stubGithub(page, {
			readmes: { "/alice/alpha/main/README.md": README },
			releases: { "alice/alpha": [[ZIP("alpha.zip", 40), ZIP("alpha.streamDeckPlugin", 2)], [ZIP("old.zip", 100)]] },
		});
		await openStore(page);
	});

	test("shows the plugin name, author and icon", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		const dialog = page.getByTestId("plugin-details");
		await expect(page.getByTestId("plugin-details-name")).toHaveText("Store Alpha");
		await expect(dialog.getByRole("img", { name: "Store Alpha" })).toHaveAttribute("src", "https://openactionapi.github.io/plugins/icons/com.example.store-one.png");
		await expect(page.getByTestId("plugin-details-author")).toHaveAttribute("href", "https://github.com/alice");
		// The author name differs from the repository owner in case, so both are shown.
		await expect(page.getByTestId("plugin-details-author")).toContainText("Alice (alice)");
	});

	test("renders the README as sanitised markdown", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		const readme = page.getByTestId("plugin-details-readme");
		await expect(readme.getByRole("heading", { name: "Alpha Plugin" })).toBeVisible();
		await expect(readme.locator("strong")).toHaveText("bold");
		await expect(readme.locator("script")).toHaveCount(0);
		expect(await page.evaluate(() => (window as any).__pwned)).toBeUndefined();
		await expect(readme.locator("img")).not.toHaveAttribute("onerror", /./);
	});

	test("README links open in a new tab and relative links resolve against the README location", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		const readme = page.getByTestId("plugin-details-readme");
		const external = readme.getByRole("link", { name: "External" });
		await expect(external).toHaveAttribute("href", "https://example.com/page");
		await expect(external).toHaveAttribute("target", "_blank");
		await expect(external).toHaveAttribute("rel", "noreferrer");
		await expect(readme.getByRole("link", { name: "relative guide" })).toHaveAttribute("href", `${RAW}/alice/alpha/main/docs/guide.md`);
	});

	test("shows the loading placeholder, then the summed download count of every release", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		await expect(page.getByTestId("plugin-details-downloads")).toHaveText("142");
	});

	test("the author link asks the backend to open the profile", async ({ page, tauri }) => {
		await openDetails(page, "Store Alpha");
		await page.getByTestId("plugin-details-author").click();
		await expect.poll(async () => (await tauri.calls("open_url")).map((call) => call.args.url)).toContain("https://github.com/alice");
	});

	test("the release button opens the explicit download URL when there is one", async ({ page, tauri }) => {
		await openDetails(page, "Store Alpha");
		await page.getByTestId("plugin-details-open-release").click();
		expect((await tauri.calls("open_url")).at(-1)!.args).toEqual({ url: "https://example.invalid/alpha.zip" });
	});

	test("the release button otherwise opens the repository's latest release", async ({ page, tauri }) => {
		await openDetails(page, "Store Beta");
		await page.getByTestId("plugin-details-open-release").click();
		expect((await tauri.calls("open_url")).at(-1)!.args).toEqual({ url: "https://github.com/bob/beta/releases/latest" });
	});

	test("closes with the close button", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		await page.getByTestId("plugin-details-close").click();
		await expect(page.getByTestId("plugin-details")).toHaveCount(0);
		await expect(page.getByTestId("plugin-manager")).toBeVisible();
	});

	test("closes by clicking the backdrop", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		await page
			.getByTestId("plugin-details")
			.getByRole("button", { name: "Close modal" })
			.click({ position: { x: 5, y: 5 } });
		await expect(page.getByTestId("plugin-details")).toHaveCount(0);
		await expect(page.getByTestId("plugin-manager")).toBeVisible();
	});

	test("Escape closes the details first and the manager second", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("plugin-details")).toHaveCount(0);
		await expect(page.getByTestId("plugin-manager")).toBeVisible();
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("plugin-manager")).toHaveCount(0);
	});

	test("is a full-screen modal dialog", async ({ page }) => {
		await openDetails(page, "Store Alpha");
		const dialog = page.getByTestId("plugin-details");
		await expect(dialog).toHaveAttribute("role", "dialog");
		await expect(dialog).toHaveAttribute("aria-modal", "true");
		await expect(dialog).toHaveClass(/modal-open/);
	});
});

test.describe("PluginDetails: README fallbacks", () => {
	test("falls back to master/readme.md when main/README.md is missing", async ({ page }) => {
		await stubGithub(page, { readmes: { "/bob/beta/master/readme.md": "# Beta on master" }, releases: { "bob/beta": [] } });
		await openStore(page);
		await openDetails(page, "Store Beta");
		await expect(page.getByTestId("plugin-details-readme").getByRole("heading", { name: "Beta on master" })).toBeVisible();
	});

	test("tries the four README locations in order", async ({ page }) => {
		await stubGithub(page, { readmes: { "/bob/beta/master/README.md": "# Third" }, releases: { "bob/beta": [] } });
		const requested: string[] = [];
		page.on("request", (request) => request.url().startsWith(RAW) && requested.push(request.url().slice(RAW.length)));
		await openStore(page);
		await openDetails(page, "Store Beta");
		await expect(page.getByTestId("plugin-details-readme").getByRole("heading", { name: "Third" })).toBeVisible();
		expect(requested).toEqual(["/bob/beta/main/README.md", "/bob/beta/main/readme.md", "/bob/beta/master/README.md"]);
	});

	test("shows a not-found notice with a GitHub link when no README exists", async ({ page }) => {
		await stubGithub(page, { releases: { "bob/beta": [] } });
		await openStore(page);
		await openDetails(page, "Store Beta");
		const readme = page.getByTestId("plugin-details-readme");
		await expect(readme).toContainText("Plugin README file not found");
		await expect(readme.getByRole("link", { name: "View plugin on GitHub" })).toHaveAttribute("href", "https://github.com/bob/beta");
		await expect(readme.getByRole("link", { name: "View plugin on GitHub" })).toHaveAttribute("target", "_blank");
	});

	test("no download badge when there are no releases", async ({ page }) => {
		await stubGithub(page, { releases: { "bob/beta": [] } });
		await openStore(page);
		await openDetails(page, "Store Beta");
		await expect(page.getByTestId("plugin-details-readme")).toContainText("Plugin README file not found");
		await expect(page.getByTestId("plugin-details-downloads")).toHaveCount(0);
	});

	test("the repository owner is appended only when it differs from the author", async ({ page }) => {
		await stubGithub(page, { releases: { "carol/same": [] } });
		await page.route("https://openactionapi.github.io/plugins/catalogue.json", (route) =>
			route.fulfill({
				json: {
					"com.example.same": { name: "Same Author", author: "carol", repository: "https://github.com/carol/same" },
					"com.example.diff": { name: "Other Author", author: "Carol D.", repository: "https://github.com/carol/same" },
				},
			}),
		);
		await openStore(page);
		await openDetails(page, "Same Author");
		await expect(page.getByTestId("plugin-details-author")).toHaveText("carol");
		await page.getByTestId("plugin-details-close").click();
		await openDetails(page, "Other Author");
		await expect(page.getByTestId("plugin-details-author")).toContainText("Carol D. (carol)");
	});
});

test.describe("Plugin installation from the store", () => {
	test("a plugin with a download URL installs directly after confirmation", async ({ page, tauri }) => {
		await stubGithub(page, { releases: { "alice/alpha": [] } });
		await openStore(page);
		await openDetails(page, "Store Alpha");
		await page.getByTestId("plugin-details-install").click();
		await expect.poll(async () => (await tauri.calls("install_plugin")).length).toBe(1);
		expect((await tauri.calls("install_plugin"))[0]!.args).toEqual({ url: "https://example.invalid/alpha.zip", file: null, fallback_id: "com.example.store-one" });
		const [ask] = await dialogCalls(tauri, "ask");
		expect(ask!.title).toBe('Install "Store Alpha"?');
		await expect.poll(async () => (await dialogCalls(tauri, "message")).at(-1)?.title).toBe('Installed "Store Alpha"');
		expect((await dialogCalls(tauri, "message")).at(-1)!.message).toBe('Successfully installed "Store Alpha".');
	});

	test("declining the confirmation installs nothing", async ({ page, tauri }) => {
		await stubGithub(page, { releases: { "alice/alpha": [] } });
		await openStore(page);
		await decline(tauri);
		await openDetails(page, "Store Alpha");
		await page.getByTestId("plugin-details-install").click();
		await expect.poll(async () => (await dialogCalls(tauri, "ask")).length).toBe(1);
		expect(await tauri.calls("install_plugin")).toHaveLength(0);
		expect(await dialogCalls(tauri, "message")).toHaveLength(0);
	});

	test("a successful install refreshes the installed list and the action library", async ({ page, tauri }) => {
		await stubGithub(page, { releases: { "alice/alpha": [] } });
		await openStore(page);
		const before = {
			plugins: (await tauri.calls("list_plugins")).length,
			categories: (await tauri.calls("get_categories")).length,
		};
		await openDetails(page, "Store Alpha");
		await page.getByTestId("plugin-details-install").click();
		await expect.poll(async () => (await tauri.calls("get_categories")).length).toBeGreaterThan(before.categories);
		await expect.poll(async () => (await tauri.calls("list_plugins")).length).toBeGreaterThan(before.plugins);
	});

	test("a backend failure is reported with the error message", async ({ page, tauri }) => {
		await stubGithub(page, { releases: { "alice/alpha": [] } });
		await openStore(page);
		await tauri.fail("install_plugin", "archive is corrupt");
		await openDetails(page, "Store Alpha");
		await page.getByTestId("plugin-details-install").click();
		await expect.poll(async () => (await dialogCalls(tauri, "message")).at(-1)).toMatchObject({ title: 'Failed to install "Store Alpha"', message: "archive is corrupt" });
	});

	test("a plugin without a download URL fetches its releases and installs the only candidate", async ({ page, tauri }) => {
		await stubGithub(page, { releases: { "bob/beta": [[ZIP("beta.zip"), { name: "notes.txt", browser_download_url: "https://downloads.invalid/notes.txt" }]] } });
		await openStore(page);
		await openDetails(page, "Store Beta");
		await page.getByTestId("plugin-details-install").click();
		await expect.poll(async () => (await tauri.calls("install_plugin")).length).toBe(1);
		expect((await tauri.calls("install_plugin"))[0]!.args).toEqual({ url: "https://downloads.invalid/beta.zip", file: null, fallback_id: "com.example.store-two" });
		await expect(page.getByTestId("asset-chooser")).toHaveCount(0);
	});

	test("the release lookup uses the GitHub API for the repository", async ({ page }) => {
		await stubGithub(page, { releases: { "bob/beta": [[ZIP("beta.zip")]] } });
		const requested: string[] = [];
		page.on("request", (request) => request.url().startsWith("https://api.github.com/") && requested.push(request.url()));
		await openStore(page);
		await openDetails(page, "Store Beta");
		await page.getByTestId("plugin-details-install").click();
		await expect.poll(() => requested.length).toBeGreaterThan(1);
		expect(new Set(requested)).toEqual(new Set(["https://api.github.com/repos/bob/beta/releases"]));
	});

	test("an unreachable release API is reported and nothing is installed", async ({ page, tauri }) => {
		await stubGithub(page, { releases: { "bob/beta": "abort" } });
		await openStore(page);
		await openDetails(page, "Store Beta");
		await page.getByTestId("plugin-details-install").click();
		await expect.poll(async () => (await dialogCalls(tauri, "message")).at(-1)?.title).toBe('Failed to install "Store Beta"');
		expect(await tauri.calls("install_plugin")).toHaveLength(0);
	});

	test("an Elgato archive plugin installs from the rezipped archive", async ({ page, tauri }) => {
		await stubGithub(page);
		await openStore(page);
		await expect(archiveItem(page, "Archive Gamma")).toBeVisible();
		await archiveItem(page, "Archive Gamma").getByTestId("plugin-action").click();
		await expect.poll(async () => (await tauri.calls("install_plugin")).length).toBe(1);
		expect((await tauri.calls("install_plugin"))[0]!.args).toEqual({ url: "https://plugins.amankhanna.me/rezipped/archive.gamma.zip", file: null, fallback_id: "archive.gamma" });
		expect((await dialogCalls(tauri, "ask"))[0]!.title).toBe('Install "Archive Gamma"?');
	});

	test("Install from file installs the picked file without asking", async ({ page, tauri }) => {
		await stubGithub(page);
		await openStore(page);
		await tauri.mockState(`(s) => { s.dialogOpen = "C:\\\\Downloads\\\\my-plugin.zip"; }`);
		await page.getByTestId("plugins-install-file").click();
		await expect.poll(async () => (await tauri.calls("install_plugin")).length).toBe(1);
		expect((await tauri.calls("install_plugin"))[0]!.args).toEqual({ url: null, file: "C:\\Downloads\\my-plugin.zip", fallback_id: null });
		expect(await dialogCalls(tauri, "ask")).toHaveLength(0);
		await expect.poll(async () => (await dialogCalls(tauri, "message")).at(-1)?.title).toBe('Installed "my-plugin.zip"');
		expect((await tauri.calls("plugin:dialog|open"))[0]!.args).toMatchObject({ options: { multiple: false, directory: false } });
	});

	test("cancelling the file picker does nothing", async ({ page, tauri }) => {
		await stubGithub(page);
		await openStore(page);
		await page.getByTestId("plugins-install-file").click();
		await expect.poll(async () => (await tauri.calls("plugin:dialog|open")).length).toBe(1);
		expect(await tauri.calls("install_plugin")).toHaveLength(0);
	});

	test("a plugin://installPlugin deep link installs the matching catalogue entry", async ({ page, tauri }) => {
		await stubGithub(page);
		await openStore(page);
		await expect.poll(() => tauri.listenerCount("deep-link://new-url")).toBeGreaterThan(0);
		await tauri.emit("deep-link://new-url", ["opendeck://installPlugin/com.example.store-one"]);
		await expect.poll(async () => (await tauri.calls("install_plugin")).length).toBe(1);
		expect((await tauri.calls("install_plugin"))[0]!.args).toMatchObject({ url: "https://example.invalid/alpha.zip", fallback_id: "com.example.store-one" });
	});

	test("deep links for unknown plugins or other actions are ignored", async ({ page, tauri }) => {
		await stubGithub(page);
		await openStore(page);
		await expect.poll(() => tauri.listenerCount("deep-link://new-url")).toBeGreaterThan(0);
		await tauri.emit("deep-link://new-url", ["opendeck://installPlugin/com.example.unknown"]);
		await tauri.emit("deep-link://new-url", ["opendeck://somethingElse/com.example.store-one"]);
		await page.waitForTimeout(300);
		expect(await tauri.calls("install_plugin")).toHaveLength(0);
	});
});

test.describe("ReleaseAssetChooser", () => {
	const assets = [ZIP("beta-linux.zip"), { name: "beta-readme.txt", browser_download_url: "https://downloads.invalid/readme.txt" }, ZIP("beta-windows.streamDeckPlugin"), ZIP("beta-mac.zip")];

	test.beforeEach(async ({ page }) => {
		await stubGithub(page, { releases: { "bob/beta": [assets] } });
		await openStore(page);
		await openDetails(page, "Store Beta");
		await page.getByTestId("plugin-details-install").click();
		await expect(page.getByTestId("asset-chooser")).toBeVisible();
	});

	test("lists only installable assets (.zip and .streamDeckPlugin)", async ({ page }) => {
		await expect(page.getByTestId("asset-chooser")).toContainText("Choose a release asset");
		await expect(page.getByTestId("asset-chooser-select").locator("option")).toHaveText(["beta-linux.zip", "beta-windows.streamDeckPlugin", "beta-mac.zip"]);
	});

	test("installs the first asset by default", async ({ page, tauri }) => {
		await page.getByTestId("asset-chooser-install").click();
		await expect(page.getByTestId("asset-chooser")).toHaveCount(0);
		await expect.poll(async () => (await tauri.calls("install_plugin")).length).toBe(1);
		expect((await tauri.calls("install_plugin"))[0]!.args).toEqual({ url: "https://downloads.invalid/beta-linux.zip", file: null, fallback_id: "com.example.store-two" });
	});

	test("installs the selected asset", async ({ page, tauri }) => {
		await page.getByTestId("asset-chooser-select").selectOption({ label: "beta-mac.zip" });
		await page.getByTestId("asset-chooser-install").click();
		await expect.poll(async () => (await tauri.calls("install_plugin")).length).toBe(1);
		expect((await tauri.calls("install_plugin"))[0]!.args.url).toBe("https://downloads.invalid/beta-mac.zip");
	});

	test("Cancel installs nothing and keeps the details open", async ({ page, tauri }) => {
		await page.getByTestId("asset-chooser-cancel").click();
		await expect(page.getByTestId("asset-chooser")).toHaveCount(0);
		await expect(page.getByTestId("plugin-details")).toBeVisible();
		expect(await tauri.calls("install_plugin")).toHaveLength(0);
		expect(await dialogCalls(tauri, "ask")).toHaveLength(0);
	});

	test("Escape cancels the chooser before closing anything else", async ({ page, tauri }) => {
		await page.keyboard.press("Escape");
		await expect(page.getByTestId("asset-chooser")).toHaveCount(0);
		await expect(page.getByTestId("plugin-details")).toBeVisible();
		await expect(page.getByTestId("plugin-manager")).toBeVisible();
		expect(await tauri.calls("install_plugin")).toHaveLength(0);
	});

	test("can be reopened after cancelling", async ({ page }) => {
		await page.getByTestId("asset-chooser-cancel").click();
		await page.getByTestId("plugin-details-install").click();
		await expect(page.getByTestId("asset-chooser")).toBeVisible();
		await expect(page.getByTestId("asset-chooser-select").locator("option")).toHaveCount(3);
	});

	test("is a modal above the plugin details", async ({ page }) => {
		const z = await page.evaluate(() => ({
			chooser: Number(getComputedStyle(document.querySelector('[data-testid="asset-chooser"]')!).zIndex),
			details: Number(getComputedStyle(document.querySelector('[data-testid="plugin-details"]')!).zIndex),
		}));
		expect(z.chooser).toBeGreaterThan(z.details);
	});
});
