import { expect, test } from "@playwright/test";

import { isGifImageSource } from "$lib/imageFormat";

test.describe("isGifImageSource", () => {
	test("rejects undefined and empty", () => {
		expect(isGifImageSource(undefined)).toBe(false);
		expect(isGifImageSource("")).toBe(false);
	});

	test("accepts gif data URLs, with and without parameters, any case, padded", () => {
		expect(isGifImageSource("data:image/gif;base64,R0lGOD")).toBe(true);
		expect(isGifImageSource("DATA:IMAGE/GIF;base64,R0lGOD")).toBe(true);
		expect(isGifImageSource("  data:image/gif;charset=utf-8;base64,AAAA ")).toBe(true);
		expect(isGifImageSource("data:image/gif,GIF89a")).toBe(true);
	});

	test("rejects other data URL types, including look-alikes", () => {
		expect(isGifImageSource("data:image/png;base64,AAAA")).toBe(false);
		expect(isGifImageSource("data:image/gifx;base64,AAAA")).toBe(false);
	});

	test("accepts .gif paths and URLs, ignoring query and hash", () => {
		expect(isGifImageSource("anim.gif")).toBe(true);
		expect(isGifImageSource("ANIM.GIF")).toBe(true);
		expect(isGifImageSource("http://127.0.0.1:57118/a/b.gif?x=1")).toBe(true);
		expect(isGifImageSource("a.gif#frag")).toBe(true);
		expect(isGifImageSource("a.gif?x=1#frag")).toBe(true);
	});

	test("rejects non-gif paths, and gif only in query/hash", () => {
		expect(isGifImageSource("a.png")).toBe(false);
		expect(isGifImageSource("a.gifx")).toBe(false);
		expect(isGifImageSource("a.png?file=b.gif")).toBe(false);
		expect(isGifImageSource("a.png#b.gif")).toBe(false);
	});
});
