import { expect, test } from "@playwright/test";

import { CanvasLock, getImage } from "$lib/rendererHelper";
import { sleep } from "./helpers/fresh";

// The shared ports module defaults to 57116 (webserver 57118) because these tests never call initPortBase.
const server = (path: string) => `http://127.0.0.1:57118/${path}`;

test.describe("getImage", () => {
	test("no image and no fallback gives the alert icon", () => {
		expect(getImage(undefined, undefined)).toBe("/alert.png");
		expect(getImage("", undefined)).toBe("/alert.png");
	});

	test("no image uses the (resolved) fallback", () => {
		expect(getImage(undefined, "fallback.png")).toBe(server("fallback.png"));
		expect(getImage("", "opendeck/fb.png")).toBe("/fb.png");
	});

	test("opendeck/ paths resolve to bundled app assets", () => {
		expect(getImage("opendeck/icon.png", undefined)).toBe("/icon.png");
	});

	test("other relative paths are served from the plugin webserver", () => {
		expect(getImage("plugin/icon.png", undefined)).toBe(server("plugin/icon.png"));
	});

	test("valid base64 data URLs pass through", () => {
		expect(getImage("data:image/png;base64,AAAA", undefined)).toBe("data:image/png;base64,AAAA");
		expect(getImage("data:image/svg+xml;base64,PHN2Zz4=", undefined)).toBe("data:image/svg+xml;base64,PHN2Zz4=");
	});

	test("base64 data URLs are trimmed to the matched payload", () => {
		expect(getImage("data:image/png;base64,AAAA!!junk", undefined)).toBe("data:image/png;base64,AAAA");
	});

	test("a base64 data URL with an empty payload falls back or alerts", () => {
		expect(getImage("data:image/png;base64,", undefined)).toBe("/alert.png");
		expect(getImage("data:image/png;base64,", "f.png")).toBe(server("f.png"));
	});

	test("non-image data URLs pass through untouched", () => {
		expect(getImage("data:text/plain,hi", undefined)).toBe("data:text/plain,hi");
	});

	test("raw SVG data URLs are re-encoded", () => {
		expect(getImage("data:image/svg+xml,<svg/>", undefined)).toBe("data:image/svg+xml,%3Csvg%2F%3E");
		expect(getImage("data:image/svg+xml;utf8,<svg/>;", undefined)).toBe("data:image/svg+xml,%3Csvg%2F%3E");
	});

	test("already percent-encoded SVG is decoded then encoded once (idempotent)", () => {
		expect(getImage("data:image/svg+xml,%3Csvg%2F%3E", undefined)).toBe("data:image/svg+xml,%3Csvg%2F%3E");
	});

	test("malformed percent-encoding in an SVG data URL falls back to the raw text", () => {
		expect(getImage("data:image/svg+xml,<svg>%E0%A4%A</svg>", undefined)).toBe("data:image/svg+xml," + encodeURIComponent("<svg>%E0%A4%A</svg>"));
	});
});

test.describe("CanvasLock", () => {
	test("lock() resolves immediately for the first holder and returns an unlock function", async () => {
		const lock = new CanvasLock();
		const unlock = await lock.lock();
		expect(typeof unlock).toBe("function");
		unlock();
	});

	test("serialises holders in FIFO order", async () => {
		const lock = new CanvasLock();
		const order: string[] = [];
		const run = async (name: string, ms: number) => {
			const unlock = await lock.lock();
			order.push(`${name}:start`);
			await sleep(ms);
			order.push(`${name}:end`);
			unlock();
		};
		await Promise.all([run("a", 20), run("b", 1), run("c", 1)]);
		expect(order).toEqual(["a:start", "a:end", "b:start", "b:end", "c:start", "c:end"]);
	});

	test("a second holder waits until the first unlocks", async () => {
		const lock = new CanvasLock();
		const unlockFirst = await lock.lock();
		let acquired = false;
		const second = lock.lock().then((unlock) => {
			acquired = true;
			return unlock;
		});
		await sleep(20);
		expect(acquired).toBe(false);
		unlockFirst();
		(await second)();
		expect(acquired).toBe(true);
	});
});
