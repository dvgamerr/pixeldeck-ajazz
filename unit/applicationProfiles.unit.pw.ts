import { expect, test } from "@playwright/test";

import { type ApplicationProfiles, DEFAULT_APPLICATION, cleanApplicationProfiles, isUnmapped, sortApplicationEntries } from "$lib/applicationProfiles";

test("DEFAULT_APPLICATION constant", () => {
	expect(DEFAULT_APPLICATION).toBe("opendeck_default");
});

test.describe("cleanApplicationProfiles", () => {
	test("drops empty profile assignments", () => {
		expect(cleanApplicationProfiles({ app: { a: "p", b: "" } })).toEqual({ app: { a: "p" } });
	});

	test("drops applications that map no device", () => {
		expect(cleanApplicationProfiles({ app: { a: "" }, empty: {}, keep: { a: "x" } })).toEqual({ keep: { a: "x" } });
	});

	test("empty input gives empty output", () => {
		expect(cleanApplicationProfiles({})).toEqual({});
	});

	test("does not mutate its input", () => {
		const input: ApplicationProfiles = { app: { a: "p", b: "" } };
		cleanApplicationProfiles(input);
		expect(input).toEqual({ app: { a: "p", b: "" } });
	});
});

test.describe("sortApplicationEntries", () => {
	test("default application sorts first, rest alphabetical", () => {
		const sorted = sortApplicationEntries({ zeta: {}, alpha: {}, [DEFAULT_APPLICATION]: {}, mid: {} });
		expect(sorted.map(([name]) => name)).toEqual([DEFAULT_APPLICATION, "alpha", "mid", "zeta"]);
	});

	test("default stays first regardless of insertion order", () => {
		const sorted = sortApplicationEntries({ a: {}, [DEFAULT_APPLICATION]: {} });
		expect(sorted[0][0]).toBe(DEFAULT_APPLICATION);
	});

	test("returns [name, devices] tuples and handles empty", () => {
		expect(sortApplicationEntries({ a: { d: "p" } })).toEqual([["a", { d: "p" }]]);
		expect(sortApplicationEntries({})).toEqual([]);
	});
});

test.describe("isUnmapped", () => {
	const profiles: ApplicationProfiles = { app: { dev: "Default", blank: "" } };

	test("true when the application is unknown", () => {
		expect(isUnmapped(profiles, "nope", "dev")).toBe(true);
	});

	test("true when the device has no entry or an empty profile", () => {
		expect(isUnmapped(profiles, "app", "other")).toBe(true);
		expect(isUnmapped(profiles, "app", "blank")).toBe(true);
	});

	test("false when mapped", () => {
		expect(isUnmapped(profiles, "app", "dev")).toBe(false);
	});
});
