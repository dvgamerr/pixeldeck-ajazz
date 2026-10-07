import { expect, test } from "@playwright/test";

import { type Context, contextsEqual } from "$lib/Context";

const base: Context = { device: "sd-1", profile: "Default", controller: "Keypad", position: 2 };

test.describe("contextsEqual", () => {
	test("same reference is equal", () => {
		expect(contextsEqual(base, base)).toBe(true);
	});

	test("structurally identical copies are equal", () => {
		expect(contextsEqual(base, { ...base })).toBe(true);
	});

	test("null/undefined: both missing differ unless identical reference", () => {
		expect(contextsEqual(null, null)).toBe(true);
		expect(contextsEqual(undefined, undefined)).toBe(true);
		expect(contextsEqual(null, undefined)).toBe(false);
		expect(contextsEqual(base, null)).toBe(false);
		expect(contextsEqual(undefined, base)).toBe(false);
	});

	for (const [field, value] of [
		["device", "sd-2"],
		["profile", "Other"],
		["controller", "Encoder"],
		["position", 3],
	] as const) {
		test(`differing ${field} is not equal`, () => {
			expect(contextsEqual(base, { ...base, [field]: value })).toBe(false);
		});
	}

	test("comparison is loose (==): numeric string position equals number", () => {
		expect(contextsEqual(base, { ...base, position: "2" as unknown as number })).toBe(true);
	});

	test("is symmetric", () => {
		const other = { ...base, position: 9 };
		expect(contextsEqual(base, other)).toBe(contextsEqual(other, base));
	});
});
