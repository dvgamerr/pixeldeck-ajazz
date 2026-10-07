import { expect, test } from "@playwright/test";

import { addToFolders, makeFolders } from "$lib/profileFolders";

test.describe("makeFolders", () => {
	test("empty list gives no folders", () => {
		expect(makeFolders([])).toEqual({});
	});

	test("ids without a slash go in the root folder ''", () => {
		expect(makeFolders(["A", "B"])).toEqual({ "": ["A", "B"] });
	});

	test("ids with a slash group under their first path segment", () => {
		expect(makeFolders(["Games/One", "Games/Two", "Work/Mail", "Root"])).toEqual({
			Games: ["Games/One", "Games/Two"],
			Work: ["Work/Mail"],
			"": ["Root"],
		});
	});

	test("nested paths only use the first segment as folder", () => {
		expect(makeFolders(["a/b/c"])).toEqual({ a: ["a/b/c"] });
	});

	test("duplicate ids are collapsed", () => {
		expect(makeFolders(["x/y", "x/y"])).toEqual({ x: ["x/y"] });
	});

	test("leading slash puts the id in the root folder", () => {
		expect(makeFolders(["/odd"])).toEqual({ "": ["/odd"] });
	});
});

test.describe("addToFolders", () => {
	test("creates the folder when missing and mutates in place", () => {
		const folders = {};
		addToFolders(folders, "F/p");
		expect(folders).toEqual({ F: ["F/p"] });
	});

	test("appends to an existing folder, skips duplicates", () => {
		const folders = { F: ["F/a"] };
		addToFolders(folders, "F/b");
		addToFolders(folders, "F/b");
		expect(folders).toEqual({ F: ["F/a", "F/b"] });
	});

	test("returns undefined", () => {
		expect(addToFolders({}, "x")).toBeUndefined();
	});
});
