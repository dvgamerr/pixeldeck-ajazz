import { expect, test } from "@playwright/test";
import { get } from "svelte/store";
import { readFileSync } from "node:fs";

import { freshImport } from "./helpers/fresh";

test.describe("singletons", () => {
	test("PRODUCT_NAME is the trimmed contents of product_name.txt", async () => {
		const { PRODUCT_NAME } = await freshImport("singletons.ts");
		const expected = readFileSync(new URL("../product_name.txt", import.meta.url), "utf8").trim();
		expect(PRODUCT_NAME).toBe(expected);
		expect(PRODUCT_NAME.length).toBeGreaterThan(0);
		expect(PRODUCT_NAME).toBe(PRODUCT_NAME.trim());
	});

	test("actionList, deviceSelector and profileManager are writable stores initialised to null", async () => {
		const mod = await freshImport("singletons.ts");
		for (const name of ["actionList", "deviceSelector", "profileManager"]) {
			const store = mod[name];
			expect(get(store), name).toBeNull();
			const fake = { name };
			store.set(fake);
			expect(get(store)).toBe(fake);
			store.set(null);
			expect(get(store)).toBeNull();
		}
	});

	test("each store is independent", async () => {
		const mod = await freshImport("singletons.ts");
		mod.actionList.set({ a: 1 });
		expect(get(mod.deviceSelector)).toBeNull();
		expect(get(mod.profileManager)).toBeNull();
	});
});
