import { expect, test } from "../fixtures/tauriMock";
import { openForModules } from "./support";

const MOD = "/src/lib/rendererHelper.ts";
const GIF = "data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==";

test.beforeEach(async ({ page }) => {
	await openForModules(page);
});

test.describe("getImage", () => {
	test("falls back to the alert icon when there is neither an image nor a fallback", async ({ page }) => {
		const results = await page.evaluate(async (mod) => {
			const { getImage } = await import(mod);
			return [getImage(undefined, undefined), getImage("", undefined), getImage("", "")];
		}, MOD);
		expect(results).toEqual(["/alert.png", "/alert.png", "/alert.png"]);
	});

	test("resolves the fallback through the plugin web server when the image is empty", async ({ page }) => {
		const result = await page.evaluate(async (mod) => (await import(mod)).getImage("", "plugin/fallback.png"), MOD);
		expect(result).toBe("http://127.0.0.1:57118/plugin/fallback.png");
	});

	test("maps opendeck/ paths onto bundled static assets", async ({ page }) => {
		const result = await page.evaluate(async (mod) => (await import(mod)).getImage("opendeck/ok.png", "x.png"), MOD);
		expect(result).toBe("/ok.png");
	});

	test("serves relative plugin paths from the plugin web server", async ({ page }) => {
		const result = await page.evaluate(async (mod) => (await import(mod)).getImage("com.example/icon.png", undefined), MOD);
		expect(result).toBe("http://127.0.0.1:57118/com.example/icon.png");
	});

	test("returns valid base64 data URLs unchanged for every supported image type", async ({ page }) => {
		const types = ["apng", "avif", "gif", "jpeg", "png", "svg+xml", "webp", "bmp", "x-icon", "tiff"];
		const outputs = await page.evaluate(
			async ([mod, list]) => {
				const { getImage } = await import(mod!);
				return (list as string[]).map((type) => [`data:image/${type};base64,QUJDRA==`, getImage(`data:image/${type};base64,QUJDRA==`, undefined)]);
			},
			[MOD, types] as const,
		);
		for (const [input, output] of outputs) expect(output).toBe(input);
	});

	test("trims trailing garbage after the base64 payload", async ({ page }) => {
		const result = await page.evaluate(async (mod) => (await import(mod)).getImage("data:image/png;base64,QUJDRA== trailing", undefined), MOD);
		expect(result).toBe("data:image/png;base64,QUJDRA==");
	});

	test("a base64 data URL without a payload uses the fallback or the alert icon", async ({ page }) => {
		const results = await page.evaluate(async (mod) => {
			const { getImage } = await import(mod);
			return [getImage("data:image/png;base64,", undefined), getImage("data:image/png;base64,", "fb.png")];
		}, MOD);
		expect(results).toEqual(["/alert.png", "http://127.0.0.1:57118/fb.png"]);
	});

	test("passes unrecognised data URLs through", async ({ page }) => {
		const result = await page.evaluate(async (mod) => (await import(mod)).getImage("data:text/plain,hello", undefined), MOD);
		expect(result).toBe("data:text/plain,hello");
	});

	test("normalises raw and percent-encoded SVG data URLs", async ({ page }) => {
		const results = await page.evaluate(async (mod) => {
			const { getImage } = await import(mod);
			const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="red"/></svg>';
			const normalised = "data:image/svg+xml," + encodeURIComponent(svg);
			return {
				normalised,
				raw: getImage("data:image/svg+xml;utf8," + svg, undefined),
				encoded: getImage("data:image/svg+xml;charset=utf8," + encodeURIComponent(svg), undefined),
				trailingSemicolon: getImage("data:image/svg+xml," + svg + ";", undefined),
			};
		}, MOD);
		expect(results.raw).toBe(results.normalised);
		expect(results.encoded).toBe(results.normalised);
		expect(results.trailingSemicolon).toBe(results.normalised);
	});

	test("the normalised SVG data URL is a decodable image", async ({ page }) => {
		const size = await page.evaluate(async (mod) => {
			const { getImage } = await import(mod);
			const url = getImage('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg" width="12" height="7"/>', undefined);
			const image = new Image();
			image.src = url;
			await image.decode();
			return [image.naturalWidth, image.naturalHeight];
		}, MOD);
		expect(size).toEqual([12, 7]);
	});

	test("an SVG data URL containing a literal percent sign does not throw", async ({ page }) => {
		// decodeURIComponent throws on a lone '%'; getImage uses try/finally without a catch, so the URIError escapes.
		test.fail(true, "getImage rethrows URIError for malformed percent-encoding in SVG data URLs (try/finally without catch)");
		const outcome = await page.evaluate(async (mod) => {
			const { getImage } = await import(mod);
			try {
				return getImage('data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg"><text>100%</text></svg>', undefined);
			} catch (error) {
				return `threw ${(error as Error).name}`;
			}
		}, MOD);
		expect(outcome).toMatch(/^data:image\/svg\+xml,/);
	});

	test("a multi-line raw SVG data URL keeps all of its content", async ({ page }) => {
		// The regex capture group uses `.+`, which stops at the first newline.
		test.fail(true, "getImage truncates raw SVG data URLs at the first newline");
		const result = await page.evaluate(async (mod) => {
			const { getImage } = await import(mod);
			return decodeURIComponent(getImage('data:image/svg+xml;utf8,<svg xmlns="http://www.w3.org/2000/svg">\n<rect/>\n</svg>', undefined).split(",")[1]!);
		}, MOD);
		expect(result).toContain("</svg>");
	});
});

test.describe("CanvasLock", () => {
	test("serialises critical sections in request order", async ({ page }) => {
		const log = await page.evaluate(async (mod) => {
			const { CanvasLock } = await import(mod);
			const lock = new CanvasLock();
			const log: string[] = [];
			let running = 0;
			let maxRunning = 0;
			const task = async (name: string, delay: number) => {
				const unlock = await lock.lock();
				running++;
				maxRunning = Math.max(maxRunning, running);
				log.push(`start ${name}`);
				await new Promise((resolve) => setTimeout(resolve, delay));
				log.push(`end ${name}`);
				running--;
				unlock();
			};
			await Promise.all([task("a", 30), task("b", 5), task("c", 1)]);
			return { log, maxRunning };
		}, MOD);
		expect(log.maxRunning).toBe(1);
		expect(log.log).toEqual(["start a", "end a", "start b", "end b", "start c", "end c"]);
	});

	test("an uncontended lock is granted immediately and can be re-acquired after unlock", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { CanvasLock } = await import(mod);
			const lock = new CanvasLock();
			const first = await lock.lock();
			let second = false;
			const pending = lock.lock().then((unlock: () => void) => {
				second = true;
				return unlock;
			});
			await new Promise((resolve) => setTimeout(resolve, 20));
			const beforeUnlock = second;
			first();
			(await pending)();
			return { beforeUnlock, afterUnlock: second };
		}, MOD);
		expect(result).toEqual({ beforeUnlock: false, afterUnlock: true });
	});
});

test.describe("renderImage", () => {
	test("draws the state image scaled over the whole canvas", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const canvas = document.createElement("canvas");
			canvas.width = 144;
			canvas.height = 144;
			const rendered = await renderImage({ canvas, state: t.state({ image: t.solid("#ff0000", 4, 4) }), processImage: true });
			return { iconUnavailable: rendered.iconUnavailable, hasImage: !!rendered.image, centre: t.px(canvas, 72, 72), corner: t.px(canvas, 1, 1), far: t.px(canvas, 142, 142) };
		}, MOD);
		expect(result.iconUnavailable).toBe(false);
		expect(result.hasImage).toBe(true);
		for (const pixel of [result.centre, result.corner, result.far]) expect(pixel).toEqual([255, 0, 0, 255]);
	});

	test("uses the fallback image when the state has none", async ({ page }) => {
		const centre = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const canvas = document.createElement("canvas");
			canvas.width = 144;
			canvas.height = 144;
			await renderImage({ canvas, state: t.state({ image: "" }), fallback: t.solid("#0000ff"), processImage: true });
			return t.px(canvas, 72, 72);
		}, MOD);
		expect(centre).toEqual([0, 0, 255, 255]);
	});

	test("without processImage the raw state image is used", async ({ page }) => {
		const centre = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const canvas = document.createElement("canvas");
			canvas.width = 144;
			canvas.height = 144;
			await renderImage({ canvas, state: t.state({ image: t.solid("#00ff00") }) });
			return t.px(canvas, 72, 72);
		}, MOD);
		expect(centre).toEqual([0, 255, 0, 255]);
	});

	test("reports iconUnavailable and clears the canvas when no icon is configured", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const canvas = document.createElement("canvas");
			canvas.width = 144;
			canvas.height = 144;
			const context = canvas.getContext("2d")!;
			context.fillStyle = "#ff00ff";
			context.fillRect(0, 0, 144, 144);
			const rendered = await renderImage({ canvas, state: t.state({ image: "" }), processImage: true });
			return { iconUnavailable: rendered.iconUnavailable, image: rendered.image ?? null, opaque: t.opaque(canvas).count };
		}, MOD);
		expect(result).toEqual({ iconUnavailable: true, image: null, opaque: 0 });
	});

	test("reports iconUnavailable when the image cannot be decoded", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const canvas = document.createElement("canvas");
			canvas.width = 144;
			canvas.height = 144;
			const rendered = await renderImage({ canvas, state: t.state({ image: "data:image/png;base64,QUJDRA==" }), processImage: true });
			return { iconUnavailable: rendered.iconUnavailable, opaque: t.opaque(canvas).count };
		}, MOD);
		expect(result).toEqual({ iconUnavailable: true, opaque: 0 });
	});

	test("a provided sourceImage is drawn without loading anything", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const source = new Image();
			source.src = t.solid("#ffff00");
			await source.decode();
			const canvas = document.createElement("canvas");
			canvas.width = 144;
			canvas.height = 144;
			const rendered = await renderImage({ canvas, state: t.state({ image: "http://127.0.0.1:1/never-loaded.png" }), sourceImage: source });
			return { same: rendered.image === source, centre: t.px(canvas, 72, 72) };
		}, MOD);
		expect(result.same).toBe(true);
		expect(result.centre).toEqual([255, 255, 0, 255]);
	});

	test("without a canvas it renders into an offscreen 144x144 canvas", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const source = new Image();
			source.src = t.solid("#ff0000");
			await source.decode();
			const rendered = await renderImage({ canvas: undefined as unknown as HTMLCanvasElement, state: t.state({ image: t.solid("#ff0000") }), sourceImage: source });
			return { iconUnavailable: rendered.iconUnavailable, same: rendered.image === source };
		}, MOD);
		expect(result).toEqual({ iconUnavailable: false, same: true });
	});

	test("decoded images are cached, GIF sources are not", async ({ page }) => {
		const result = await page.evaluate(
			async ([mod, gif]) => {
				const { renderImage } = await import(mod!);
				const t = (window as any).__t;
				let created = 0;
				const original = document.createElement.bind(document);
				document.createElement = ((tag: string, options?: ElementCreationOptions) => {
					if (tag == "img") created++;
					return original(tag, options);
				}) as typeof document.createElement;
				const draw = async (image: string) => {
					const canvas = document.createElement("canvas");
					canvas.width = 20;
					canvas.height = 20;
					await renderImage({ canvas, state: t.state({ image }) });
				};
				const png = t.solid("#123456");
				await draw(png);
				await draw(png);
				const afterPng = created;
				await draw(gif!);
				await draw(gif!);
				document.createElement = original;
				return { afterPng, afterGif: created - afterPng };
			},
			[MOD, GIF] as const,
		);
		expect(result.afterPng).toBe(1);
		expect(result.afterGif).toBe(2);
	});

	test("evicts the oldest decoded image once more than 256 are cached", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			let created = 0;
			const original = document.createElement.bind(document);
			document.createElement = ((tag: string, options?: ElementCreationOptions) => {
				if (tag == "img") created++;
				return original(tag, options);
			}) as typeof document.createElement;
			const sources: string[] = [];
			for (let i = 0; i < 258; i++) sources.push(t.solid(`rgb(${i % 256}, ${Math.floor(i / 16)}, ${i % 7})`, 1 + (i % 5), 1 + Math.floor(i / 5) + 1));
			const canvas = document.createElement("canvas");
			const draw = (image: string) => renderImage({ canvas, state: t.state({ image }) });
			for (const source of sources) await draw(source);
			const afterFill = created;
			await draw(sources[257]!);
			const newestIsCached = created == afterFill;
			await draw(sources[0]!);
			const oldestWasEvicted = created == afterFill + 1;
			document.createElement = original;
			return { unique: new Set(sources).size, newestIsCached, oldestWasEvicted };
		}, MOD);
		expect(result.unique).toBe(258);
		expect(result.newestIsCached).toBe(true);
		expect(result.oldestWasEvicted).toBe(true);
	});

	test("a failed decode is not cached", async ({ page }) => {
		const created = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			let count = 0;
			const original = document.createElement.bind(document);
			document.createElement = ((tag: string, options?: ElementCreationOptions) => {
				if (tag == "img") count++;
				return original(tag, options);
			}) as typeof document.createElement;
			const canvas = original("canvas");
			for (let i = 0; i < 2; i++) await renderImage({ canvas, state: t.state({ image: "data:image/png;base64,QUJDRA==" }), processImage: true });
			document.createElement = original;
			return count;
		}, MOD);
		expect(created).toBe(2);
	});
});

test.describe("renderImage text", () => {
	const text = async (page: import("@playwright/test").Page, state: Record<string, unknown>, size = 144) =>
		page.evaluate(
			async ([mod, overrides, canvasSize]) => {
				const { renderImage } = await import(mod as string);
				const t = (window as any).__t;
				const canvas = document.createElement("canvas");
				canvas.width = canvas.height = canvasSize as number;
				await renderImage({ canvas, state: t.state({ show: true, text: "Hi", ...(overrides as object) }), processImage: true });
				return { box: t.opaque(canvas), canvasSize };
			},
			[MOD, state, size] as const,
		);

	test("draws no text unless show is set", async ({ page }) => {
		const hidden = await text(page, { show: false });
		const shown = await text(page, { show: true });
		expect(hidden.box.count).toBe(0);
		expect(shown.box.count).toBeGreaterThan(0);
	});

	test("middle alignment is centred, top and bottom hug their edges", async ({ page }) => {
		const middle = (await text(page, { alignment: "middle" })).box;
		const top = (await text(page, { alignment: "top" })).box;
		const bottom = (await text(page, { alignment: "bottom" })).box;
		const centreOf = (box: { minX: number; maxX: number }) => (box.minX + box.maxX) / 2;
		expect(Math.abs(centreOf(middle) - 72)).toBeLessThan(4);
		expect(top.minY).toBeLessThan(middle.minY);
		expect(bottom.maxY).toBeGreaterThan(middle.maxY);
		expect(top.minY).toBeLessThan(20);
		expect(bottom.maxY).toBeGreaterThan(124);
	});

	test("renders in the chosen colour with a black outline", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const canvas = document.createElement("canvas");
			canvas.width = canvas.height = 144;
			await renderImage({ canvas, state: t.state({ show: true, text: "MMM", colour: "#ff0000", size: 30 }), processImage: true });
			const { data } = canvas.getContext("2d")!.getImageData(0, 0, 144, 144);
			let red = 0,
				black = 0;
			for (let i = 0; i < data.length; i += 4) {
				if (data[i + 3] != 255) continue;
				if (data[i] == 255 && data[i + 1] == 0 && data[i + 2] == 0) red++;
				else if (data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 0) black++;
			}
			return { red, black };
		}, MOD);
		expect(result.red).toBeGreaterThan(50);
		expect(result.black).toBeGreaterThan(20);
	});

	test("a larger size produces larger text", async ({ page }) => {
		const small = (await text(page, { size: 12 })).box;
		const large = (await text(page, { size: 30 })).box;
		expect(large.maxX - large.minX).toBeGreaterThan(small.maxX - small.minX);
		expect(large.maxY - large.minY).toBeGreaterThan(small.maxY - small.minY);
	});

	test("every newline adds a line of text", async ({ page }) => {
		const one = (await text(page, { text: "Hi", alignment: "top" })).box;
		const three = (await text(page, { text: "Hi\nHi\nHi", alignment: "top" })).box;
		expect(three.maxY - three.minY).toBeGreaterThan((one.maxY - one.minY) * 2);
	});

	test("underline adds pixels beneath the text", async ({ page }) => {
		const plain = (await text(page, { underline: false })).box;
		const underlined = (await text(page, { underline: true })).box;
		expect(underlined.count).toBeGreaterThan(plain.count);
		expect(underlined.maxY).toBeGreaterThan(plain.maxY);
	});

	test("bold and italic styles change the glyphs", async ({ page }) => {
		const regular = (await text(page, { style: "Regular", text: "Hill" })).box;
		const bold = (await text(page, { style: "Bold", text: "Hill" })).box;
		const italic = (await text(page, { style: "Italic", text: "Hill" })).box;
		const both = (await text(page, { style: "Bold Italic", text: "Hill" })).box;
		expect(bold.count).toBeGreaterThan(regular.count);
		expect(italic.count).not.toBe(regular.count);
		expect(both.count).not.toBe(bold.count);
	});

	test("text scales with the canvas width", async ({ page }) => {
		const small = (await text(page, {}, 144)).box;
		const double = (await text(page, {}, 288)).box;
		expect(double.maxX - double.minX).toBeGreaterThan((small.maxX - small.minX) * 1.6);
	});
});

test.describe("renderImage overlays and press", () => {
	const draw = (page: import("@playwright/test").Page, options: Record<string, unknown>) =>
		page.evaluate(
			async ([mod, extra]) => {
				const { renderImage } = await import(mod as string);
				const t = (window as any).__t;
				const canvas = document.createElement("canvas");
				canvas.width = canvas.height = 144;
				await renderImage({ canvas, state: t.state({ image: t.solid("#ff0000") }), processImage: true, ...(extra as object) });
				const { data } = canvas.getContext("2d")!.getImageData(0, 0, 144, 144);
				let changed = 0;
				for (let i = 0; i < data.length; i += 4) if (data[i] != 255 || data[i + 1] != 0 || data[i + 2] != 0) changed++;
				return { changed, corner: t.px(canvas, 2, 2), edge: t.px(canvas, 7, 72), centre: t.px(canvas, 72, 72), inner: t.px(canvas, 22, 72) };
			},
			[MOD, options] as const,
		);

	test("the plain render leaves the icon untouched", async ({ page }) => {
		expect((await draw(page, {})).changed).toBe(0);
	});

	test("showOk draws the bundled OK overlay on top", async ({ page }) => {
		expect((await draw(page, { showOk: true })).changed).toBeGreaterThan(100);
	});

	test("showAlert draws the bundled alert overlay on top", async ({ page }) => {
		const ok = await draw(page, { showOk: true });
		const alert = await draw(page, { showAlert: true });
		expect(alert.changed).toBeGreaterThan(100);
		expect(alert.changed).not.toBe(ok.changed);
	});

	test("pressed shrinks the picture towards the centre leaving a 10% transparent margin", async ({ page }) => {
		const result = await draw(page, { pressed: true });
		expect(result.corner[3]).toBe(0);
		expect(result.edge[3]).toBe(0);
		expect(result.inner).toEqual([255, 0, 0, 255]);
		expect(result.centre).toEqual([255, 0, 0, 255]);
	});

	test("an overlay that fails to load must not hang the renderer", async ({ page }) => {
		// drawOverlay awaits overlay.onload only; a missing overlay asset leaves renderImage pending forever
		// (and any caller holding the CanvasLock stuck).
		test.fail(true, "drawOverlay never settles when /ok.png fails to load (no onerror handler)");
		await page.route("**/ok.png", (route) => route.abort());
		const settled = await page.evaluate(async (mod) => {
			const { renderImage } = await import(mod);
			const t = (window as any).__t;
			const canvas = document.createElement("canvas");
			canvas.width = canvas.height = 144;
			const outcome = await Promise.race([
				renderImage({ canvas, state: t.state({ image: t.solid("#ff0000") }), processImage: true, showOk: true }).then(() => "done"),
				new Promise((resolve) => setTimeout(() => resolve("hung"), 1500)),
			]);
			return outcome;
		}, MOD);
		expect(settled).toBe("done");
	});
});

test.describe("resizeImage", () => {
	const resize = (page: import("@playwright/test").Page, width: number, height: number) =>
		page.evaluate(
			async ([mod, w, h]) => {
				const { resizeImage } = await import(mod as string);
				const t = (window as any).__t;
				const output: string = await resizeImage(t.solid("#00ff00", w as number, h as number));
				const canvas = await t.toCanvas(output);
				return {
					prefix: output.slice(0, 22),
					width: canvas.width,
					height: canvas.height,
					centre: t.px(canvas, 144, 144),
					top: t.px(canvas, 144, 4),
					left: t.px(canvas, 4, 144),
					box: t.opaque(canvas),
				};
			},
			[MOD, width, height] as const,
		);

	test("returns GIF sources untouched so every frame survives", async ({ page }) => {
		const result = await page.evaluate(async ([mod, gif]) => (await import(mod!)).resizeImage(gif!), [MOD, GIF] as const);
		expect(result).toBe(GIF);
	});

	test("outputs a 288x288 PNG", async ({ page }) => {
		const result = await resize(page, 50, 50);
		expect(result.prefix).toBe("data:image/png;base64,");
		expect([result.width, result.height]).toEqual([288, 288]);
	});

	test("a square image fills the canvas", async ({ page }) => {
		const result = await resize(page, 40, 40);
		expect(result.box).toMatchObject({ minX: 0, minY: 0, maxX: 287, maxY: 287 });
	});

	test("a wide image is letterboxed vertically", async ({ page }) => {
		const result = await resize(page, 200, 100);
		expect(result.box.minX).toBe(0);
		expect(result.box.maxX).toBe(287);
		expect(result.box.minY).toBeGreaterThanOrEqual(71);
		expect(result.box.minY).toBeLessThanOrEqual(73);
		expect(result.box.maxY).toBeGreaterThanOrEqual(214);
		expect(result.top[3]).toBe(0);
		expect(result.centre).toEqual([0, 255, 0, 255]);
	});

	test("a tall image is pillarboxed horizontally", async ({ page }) => {
		const result = await resize(page, 100, 200);
		expect(result.box.minY).toBe(0);
		expect(result.box.maxY).toBe(287);
		expect(result.box.minX).toBeGreaterThanOrEqual(71);
		expect(result.box.minX).toBeLessThanOrEqual(73);
		expect(result.left[3]).toBe(0);
		expect(result.centre).toEqual([0, 255, 0, 255]);
	});

	test("an image that cannot be decoded settles instead of hanging", async ({ page }) => {
		// resizeImage only sets image.onload, so a corrupt file never resolves; InstanceEditor then silently ignores the chosen file.
		test.fail(true, "resizeImage never settles for an undecodable image (no onerror handler)");
		const outcome = await page.evaluate(async (mod) => {
			const { resizeImage } = await import(mod);
			const settle = resizeImage("data:image/png;base64,QUJDRA==").then(
				() => "settled",
				() => "settled",
			);
			return Promise.race([settle, new Promise((resolve) => setTimeout(() => resolve("hung"), 1500))]);
		}, MOD);
		expect(outcome).toBe("settled");
	});
});
