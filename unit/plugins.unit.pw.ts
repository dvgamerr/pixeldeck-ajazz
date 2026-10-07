import { expect, test } from "@playwright/test";

import { getFetch, isInstallableAsset, matchesQuery, releasesEndpoint, sortInstalledPlugins } from "$lib/plugins";

test.describe("getFetch", () => {
	const g = globalThis as any;
	let saved: unknown;
	test.beforeEach(() => {
		saved = g.window;
	});
	test.afterEach(() => {
		g.window = saved;
	});

	test("prefers window.fetchNative when the Tauri shell injected it", () => {
		const native = () => {};
		g.window = { fetchNative: native, fetch: () => {} };
		expect(getFetch()).toBe(native);
	});

	test("falls back to window.fetch", () => {
		const plain = () => {};
		g.window = { fetch: plain };
		expect(getFetch()).toBe(plain);
	});
});

test.describe("isInstallableAsset", () => {
	test("accepts .streamDeckPlugin and .zip case-insensitively", () => {
		expect(isInstallableAsset({ name: "a.streamDeckPlugin" })).toBe(true);
		expect(isInstallableAsset({ name: "A.STREAMDECKPLUGIN" })).toBe(true);
		expect(isInstallableAsset({ name: "a.zip" })).toBe(true);
		expect(isInstallableAsset({ name: "A.ZIP" })).toBe(true);
	});

	test("rejects other files and extensions only mid-name", () => {
		expect(isInstallableAsset({ name: "a.tar.gz" })).toBe(false);
		expect(isInstallableAsset({ name: "a.zip.sig" })).toBe(false);
		expect(isInstallableAsset({ name: "zip" })).toBe(false);
		expect(isInstallableAsset({ name: "" })).toBe(false);
	});
});

test.describe("releasesEndpoint", () => {
	test("maps a github repo URL to the API releases endpoint", () => {
		expect(releasesEndpoint("https://github.com/owner/repo").href).toBe("https://api.github.com/repos/owner/repo/releases");
	});

	test("a trailing slash in the repository URL is ignored", () => {
		expect(releasesEndpoint("https://github.com/owner/repo/").pathname).toBe("/repos/owner/repo/releases");
	});

	test("throws on an invalid repository URL", () => {
		expect(() => releasesEndpoint("not a url")).toThrow();
	});
});

test.describe("sortInstalledPlugins", () => {
	test("builtin plugins first, then alphabetical by id", () => {
		const sorted = sortInstalledPlugins([{ id: "z" }, { id: "b", builtin: true }, { id: "a" }, { id: "c", builtin: true }]);
		expect(sorted.map((p) => p.id)).toEqual(["b", "c", "a", "z"]);
	});

	test("sorts in place and returns the same array", () => {
		const input = [{ id: "b" }, { id: "a" }];
		expect(sortInstalledPlugins(input)).toBe(input);
		expect(input.map((p) => p.id)).toEqual(["a", "b"]);
	});

	test("empty array is fine", () => {
		expect(sortInstalledPlugins([])).toEqual([]);
	});
});

test.describe("matchesQuery", () => {
	test("case-insensitive substring match", () => {
		expect(matchesQuery("Stream Deck Tools", "deck")).toBe(true);
		expect(matchesQuery("abc", "ABC")).toBe(true);
	});

	test("empty query matches everything", () => {
		expect(matchesQuery("anything", "")).toBe(true);
	});

	test("non-matching query", () => {
		expect(matchesQuery("abc", "abd")).toBe(false);
	});
});
