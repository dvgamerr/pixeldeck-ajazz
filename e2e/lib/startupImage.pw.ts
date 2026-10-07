import { expect, test } from "../fixtures/tauriMock";
import { openForModules } from "./support";

const MOD = "/src/lib/startupImage.ts";

test.beforeEach(async ({ page }) => {
	await openForModules(page);
});

const SVG_OK = '<svg xmlns="http://www.w3.org/2000/svg" width="10" height="6"><rect width="10" height="6" fill="#00ff00"/></svg>';

test.describe("decodeImage", () => {
	test("resolves a loaded image with its natural size", async ({ page }) => {
		const size = await page.evaluate(async (mod) => {
			const { decodeImage } = await import(mod);
			const image = await decodeImage((window as any).__t.solid("#ff0000", 30, 12));
			return [image.naturalWidth, image.naturalHeight];
		}, MOD);
		expect(size).toEqual([30, 12]);
	});

	test("rejects with a readable error for undecodable data", async ({ page }) => {
		const message = await page.evaluate(async (mod) => {
			const { decodeImage } = await import(mod);
			try {
				await decodeImage("data:image/png;base64,QUJDRA==");
				return "resolved";
			} catch (error) {
				return (error as Error).message;
			}
		}, MOD);
		expect(message).toBe("The image could not be decoded");
	});
});

test.describe("geometry with decoded images", () => {
	test("getFittedImageSize covers the output (object-fit: cover) and applies zoom", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { decodeImage, getFittedImageSize } = await import(mod);
			const t = (window as any).__t;
			const wide = await decodeImage(t.solid("#fff", 20, 10));
			const tall = await decodeImage(t.solid("#fff", 10, 20));
			return {
				wideInSquare: getFittedImageSize(wide, { width: 100, height: 100 }, 1),
				tallInSquare: getFittedImageSize(tall, { width: 100, height: 100 }, 1),
				zoomed: getFittedImageSize(wide, { width: 100, height: 100 }, 0.5),
				upscaled: getFittedImageSize(wide, { width: 400, height: 100 }, 1),
			};
		}, MOD);
		expect(result.wideInSquare).toEqual({ width: 200, height: 100 });
		expect(result.tallInSquare).toEqual({ width: 100, height: 200 });
		expect(result.zoomed).toEqual({ width: 100, height: 50 });
		expect(result.upscaled).toEqual({ width: 400, height: 200 });
	});

	test("getTransformBounds returns normalised bounds of the fitted, offset layer", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { decodeImage, getTransformBounds } = await import(mod);
			const t = (window as any).__t;
			const decoded = await decodeImage(t.solid("#fff", 10, 10));
			const layer = { id: "a", name: "a", image: "", zoom: 1, offset_x: 0, offset_y: 0, rotation: 0, decoded };
			return {
				centred: getTransformBounds(layer, 100, 50),
				moved: getTransformBounds({ ...layer, offset_x: 50, offset_y: -25 }, 100, 50),
				halved: getTransformBounds({ ...layer, zoom: 0.5 }, 100, 100),
				noLayer: getTransformBounds(undefined, 100, 50),
				noDecoded: getTransformBounds({ ...layer, decoded: undefined }, 100, 50),
				noSize: getTransformBounds(layer, 0, 50),
			};
		}, MOD);
		const zero = { left: 0, top: 0, width: 0, height: 0 };
		expect(result.centred).toEqual({ left: 0, top: -0.5, width: 1, height: 2 });
		expect(result.moved).toEqual({ left: 0.5, top: -1, width: 1, height: 2 });
		expect(result.halved).toEqual({ left: 0.25, top: 0.25, width: 0.5, height: 0.5 });
		expect(result.noLayer).toEqual(zero);
		expect(result.noDecoded).toEqual(zero);
		expect(result.noSize).toEqual(zero);
	});

	test("rotateVector rotates counter-clockwise in screen maths and is the identity at 0", async ({ page }) => {
		const result = await page.evaluate(async (mod) => {
			const { rotateVector } = await import(mod);
			const round = (v: { x: number; y: number }) => ({ x: Math.round(v.x * 1e6) / 1e6 + 0, y: Math.round(v.y * 1e6) / 1e6 + 0 });
			return { zero: round(rotateVector(3, 4, 0)), quarter: round(rotateVector(1, 0, 90)), half: round(rotateVector(2, 1, 180)), full: round(rotateVector(2, 1, 360)) };
		}, MOD);
		expect(result.zero).toEqual({ x: 3, y: 4 });
		expect(result.quarter).toEqual({ x: 0, y: 1 });
		expect(result.half).toEqual({ x: -2, y: -1 });
		expect(result.full).toEqual({ x: 2, y: 1 });
	});
});

test.describe("drawComposedImage", () => {
	const compose = (page: import("@playwright/test").Page, scenario: string) =>
		page.evaluate(
			async ([mod, which]) => {
				const { decodeImage, drawComposedImage } = await import(mod!);
				const t = (window as any).__t;
				const red = await decodeImage(t.solid("#ff0000", 20, 20));
				const blue = await decodeImage(t.solid("#0000ff", 20, 20));
				// Half red / half blue, wide: left red, right blue.
				const split = document.createElement("canvas");
				split.width = 20;
				split.height = 10;
				const splitContext = split.getContext("2d")!;
				splitContext.fillStyle = "#ff0000";
				splitContext.fillRect(0, 0, 10, 10);
				splitContext.fillStyle = "#0000ff";
				splitContext.fillRect(10, 0, 10, 10);
				const splitImage = await decodeImage(split.toDataURL());
				const layer = (decoded: HTMLImageElement | undefined, extra = {}) => ({
					id: Math.random().toString(),
					name: "l",
					image: "",
					zoom: 1,
					offset_x: 0,
					offset_y: 0,
					rotation: 0,
					decoded,
					...extra,
				});
				const canvas = document.createElement("canvas");
				const output = { width: 100, height: 100 };
				switch (which) {
					case "empty":
						drawComposedImage(canvas, [], output);
						return { size: [canvas.width, canvas.height], centre: t.px(canvas, 50, 50) };
					case "fill":
						drawComposedImage(canvas, [layer(red)], output);
						return { corner: t.px(canvas, 1, 1), centre: t.px(canvas, 50, 50) };
					case "zoom":
						drawComposedImage(canvas, [layer(red, { zoom: 0.5 })], output);
						return { corner: t.px(canvas, 5, 5), centre: t.px(canvas, 50, 50), inside: t.px(canvas, 30, 30) };
					case "offset":
						drawComposedImage(canvas, [layer(red, { zoom: 0.5, offset_x: 25, offset_y: -25 })], output);
						return { oldCentre: t.px(canvas, 40, 60), newCentre: t.px(canvas, 75, 25) };
					case "order":
						drawComposedImage(canvas, [layer(red), layer(blue, { zoom: 0.5 })], output);
						return { centre: t.px(canvas, 50, 50), edge: t.px(canvas, 5, 5) };
					case "skip-undecoded":
						drawComposedImage(canvas, [layer(red), layer(undefined)], output);
						return { centre: t.px(canvas, 50, 50) };
					case "rotate":
						drawComposedImage(canvas, [layer(splitImage, { rotation: 90 })], output);
						return { top: t.px(canvas, 50, 8), bottom: t.px(canvas, 50, 92) };
					case "no-rotate":
						drawComposedImage(canvas, [layer(splitImage)], output);
						return { left: t.px(canvas, 8, 50), right: t.px(canvas, 92, 50) };
					case "zero-output": {
						canvas.width = 7;
						canvas.height = 9;
						drawComposedImage(canvas, [layer(red)], { width: 0, height: 100 });
						return { size: [canvas.width, canvas.height], pixel: t.px(canvas, 1, 1) };
					}
					case "resize": {
						canvas.width = 10;
						canvas.height = 10;
						drawComposedImage(canvas, [layer(red)], { width: 40, height: 20 });
						return { size: [canvas.width, canvas.height], centre: t.px(canvas, 20, 10) };
					}
				}
			},
			[MOD, scenario] as const,
		);

	test("an empty composition is an opaque black canvas of the output size", async ({ page }) => {
		const result: any = await compose(page, "empty");
		expect(result.size).toEqual([100, 100]);
		expect(result.centre).toEqual([0, 0, 0, 255]);
	});

	test("a single layer covers the whole canvas", async ({ page }) => {
		const result: any = await compose(page, "fill");
		expect(result.corner).toEqual([255, 0, 0, 255]);
		expect(result.centre).toEqual([255, 0, 0, 255]);
	});

	test("zoom shrinks the layer around the centre leaving black borders", async ({ page }) => {
		const result: any = await compose(page, "zoom");
		expect(result.corner).toEqual([0, 0, 0, 255]);
		expect(result.inside).toEqual([255, 0, 0, 255]);
		expect(result.centre).toEqual([255, 0, 0, 255]);
	});

	test("offsets move the layer in output pixels", async ({ page }) => {
		const result: any = await compose(page, "offset");
		expect(result.oldCentre).toEqual([0, 0, 0, 255]);
		expect(result.newCentre).toEqual([255, 0, 0, 255]);
	});

	test("later layers are drawn on top of earlier ones", async ({ page }) => {
		const result: any = await compose(page, "order");
		expect(result.centre).toEqual([0, 0, 255, 255]);
		expect(result.edge).toEqual([255, 0, 0, 255]);
	});

	test("layers that are not decoded yet are skipped", async ({ page }) => {
		const result: any = await compose(page, "skip-undecoded");
		expect(result.centre).toEqual([255, 0, 0, 255]);
	});

	test("rotation turns the layer clockwise in degrees", async ({ page }) => {
		const upright: any = await compose(page, "no-rotate");
		expect(upright.left).toEqual([255, 0, 0, 255]);
		expect(upright.right).toEqual([0, 0, 255, 255]);
		const rotated: any = await compose(page, "rotate");
		expect(rotated.top).toEqual([255, 0, 0, 255]);
		expect(rotated.bottom).toEqual([0, 0, 255, 255]);
	});

	test("resizes the canvas to the requested output", async ({ page }) => {
		const result: any = await compose(page, "resize");
		expect(result.size).toEqual([40, 20]);
		expect(result.centre).toEqual([255, 0, 0, 255]);
	});

	test("a zero-sized output leaves the canvas untouched", async ({ page }) => {
		const result: any = await compose(page, "zero-output");
		expect(result.size).toEqual([7, 9]);
		expect(result.pixel).toEqual([0, 0, 0, 0]);
	});
});

test.describe("readFile", () => {
	const read = (page: import("@playwright/test").Page, name: string, type: string, extension: string, content: string | number[]) =>
		page.evaluate(
			async ([mod, fileName, mime, ext, data]) => {
				const { readFile } = await import(mod as string);
				const bytes = typeof data == "string" ? data : new Uint8Array(data as number[]);
				const file = new File([bytes], fileName as string, { type: mime as string });
				try {
					return { ok: await readFile(file, ext) };
				} catch (error) {
					return { error: (error as Error).message };
				}
			},
			[MOD, name, type, extension, content] as const,
		);

	test("reads raster files as data URLs with the mime type derived from the extension", async ({ page }) => {
		const bytes = [1, 2, 3, 4];
		const png: any = await read(page, "a.png", "", "png", bytes);
		const bmp: any = await read(page, "a.bmp", "", "bmp", bytes);
		const jpg: any = await read(page, "a.jpg", "", "jpg", bytes);
		const jpeg: any = await read(page, "a.jpeg", "", "jpeg", bytes);
		expect(png.ok).toBe("data:image/png;base64,AQIDBA==");
		expect(bmp.ok).toBe("data:image/bmp;base64,AQIDBA==");
		expect(jpg.ok).toBe("data:image/jpeg;base64,AQIDBA==");
		expect(jpeg.ok).toBe("data:image/jpeg;base64,AQIDBA==");
	});

	test("the extension wins over a misleading browser-reported type", async ({ page }) => {
		const result: any = await read(page, "a.png", "image/x-ms-bmp", "png", [9]);
		expect(result.ok).toMatch(/^data:image\/png;base64,/);
	});

	test("SVG files are sanitised and re-encoded as a base64 data URL", async ({ page }) => {
		const result: any = await read(page, "ok.svg", "image/svg+xml", "svg", SVG_OK);
		expect(result.ok).toMatch(/^data:image\/svg\+xml;base64,/);
		const decoded = Buffer.from(result.ok.split(",")[1], "base64").toString("utf8");
		expect(decoded).toContain("<rect");
		expect(decoded).toContain('fill="#00ff00"');
	});

	test("a sanitised SVG still decodes as an image", async ({ page }) => {
		const size = await page.evaluate(
			async ([mod, svg]) => {
				const { readFile, decodeImage } = await import(mod!);
				const url = await readFile(new File([svg!], "ok.svg", { type: "image/svg+xml" }), "svg");
				const image = await decodeImage(url);
				return [image.naturalWidth, image.naturalHeight];
			},
			[MOD, SVG_OK] as const,
		);
		expect(size).toEqual([10, 6]);
	});

	test("scripts, event handlers, foreignObject and external embeds are stripped from SVG", async ({ page }) => {
		const dirty =
			'<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="10" height="10" onload="alert(1)">' +
			'<circle cx="5" cy="5" r="2" fill="red"/>' +
			"<script>alert(2)</script>" +
			'<foreignObject width="5" height="5"><div xmlns="http://www.w3.org/1999/xhtml">html</div></foreignObject>' +
			'<a xlink:href="javascript:alert(3)"><rect width="5" height="5" onclick="alert(4)"/></a>' +
			'<iframe src="https://example.invalid"></iframe><object data="x"></object><embed src="x"/>' +
			"</svg>";
		const result: any = await read(page, "dirty.svg", "image/svg+xml", "svg", dirty);
		expect(result.error).toBeUndefined();
		const decoded = Buffer.from(result.ok.split(",")[1], "base64").toString("utf8").toLowerCase();
		for (const forbidden of ["<script", "alert(", "onload", "onclick", "foreignobject", "javascript:", "<iframe", "<object", "<embed"]) expect(decoded, forbidden).not.toContain(forbidden);
		expect(decoded).toContain("<circle");
	});

	test("rejects files that are not SVG documents", async ({ page }) => {
		const html: any = await read(page, "page.svg", "image/svg+xml", "svg", "<html><body><p>not an image</p></body></html>");
		expect(html.error).toBe("page.svg is not a valid SVG image");
		const empty: any = await read(page, "empty.svg", "image/svg+xml", "svg", "");
		expect(empty.error).toBe("empty.svg is not a valid SVG image");
		const text: any = await read(page, "notes.svg", "image/svg+xml", "svg", "just some text");
		expect(text.error).toBe("notes.svg is not a valid SVG image");
	});

	test("large SVG documents survive the chunked base64 encoding", async ({ page }) => {
		const rects = Array.from({ length: 2000 }, (_, i) => `<rect x="${i % 100}" y="${Math.floor(i / 100)}" width="1" height="1" fill="#112233"/>`).join("");
		const big = `<svg xmlns="http://www.w3.org/2000/svg" width="100" height="20">${rects}</svg>`;
		const result: any = await read(page, "big.svg", "image/svg+xml", "svg", big);
		const decoded = Buffer.from(result.ok.split(",")[1], "base64").toString("utf8");
		expect(decoded.length).toBeGreaterThan(8192 * 2);
		expect(decoded.match(/<rect/g)).toHaveLength(2000);
	});
});

test.describe("partitionFiles", () => {
	const partition = (page: import("@playwright/test").Page, files: { name: string; type?: string; size?: number }[]) =>
		page.evaluate(
			async ([mod, list]) => {
				const { partitionFiles } = await import(mod as string);
				const made = (list as { name: string; type?: string; size?: number }[]).map(({ name, type, size }) => new File([new Uint8Array(size ?? 4)], name, { type: type ?? "" }));
				const { validFiles, rejected } = partitionFiles(made);
				return { valid: validFiles.map((entry: { file: File; extension: string }) => [entry.file.name, entry.extension]), rejected };
			},
			[MOD, files] as const,
		);

	test("accepts the supported extensions case-insensitively, with or without a reported mime type", async ({ page }) => {
		const result = await partition(page, [
			{ name: "a.png", type: "image/png" },
			{ name: "b.JPG", type: "image/jpeg" },
			{ name: "c.jpeg" },
			{ name: "d.bmp", type: "image/x-ms-bmp" },
			{ name: "e.bmp", type: "image/bmp" },
			{ name: "f.svg", type: "image/svg+xml" },
			{ name: "g.Svg" },
		]);
		expect(result.rejected).toEqual([]);
		expect(result.valid).toEqual([
			["a.png", "png"],
			["b.JPG", "jpg"],
			["c.jpeg", "jpeg"],
			["d.bmp", "bmp"],
			["e.bmp", "bmp"],
			["f.svg", "svg"],
			["g.Svg", "svg"],
		]);
	});

	test("rejects unsupported extensions, missing extensions and mismatching mime types", async ({ page }) => {
		const result = await partition(page, [
			{ name: "anim.gif", type: "image/gif" },
			{ name: "noext" },
			{ name: "doc.txt", type: "text/plain" },
			{ name: "fake.png", type: "text/html" },
			{ name: "ok.png", type: "image/png" },
		]);
		expect(result.valid).toEqual([["ok.png", "png"]]);
		expect(result.rejected).toEqual(["anim.gif has an unsupported file type", "noext has an unsupported file type", "doc.txt has an unsupported file type", "fake.png has an unsupported file type"]);
	});

	test("rejects files larger than 10 MB but accepts exactly 10 MB", async ({ page }) => {
		const limit = 10 * 1024 * 1024;
		const result = await partition(page, [
			{ name: "edge.png", type: "image/png", size: limit },
			{ name: "over.png", type: "image/png", size: limit + 1 },
		]);
		expect(result.valid).toEqual([["edge.png", "png"]]);
		expect(result.rejected).toEqual(["over.png is larger than 10 MB"]);
	});

	test("keeps the order of valid files and handles an empty list", async ({ page }) => {
		const empty = await partition(page, []);
		expect(empty).toEqual({ valid: [], rejected: [] });
	});
});

test.describe("makeLayerId", () => {
	test("returns unique UUIDs", async ({ page }) => {
		const ids = await page.evaluate(async (mod) => {
			const { makeLayerId } = await import(mod);
			return Array.from({ length: 50 }, () => makeLayerId());
		}, MOD);
		expect(new Set(ids).size).toBe(50);
		for (const id of ids) expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
	});

	test("falls back to a time-based id when crypto.randomUUID is unavailable", async ({ page }) => {
		const ids = await page.evaluate(async (mod) => {
			const { makeLayerId } = await import(mod);
			Object.defineProperty(globalThis.crypto, "randomUUID", { value: undefined, configurable: true });
			return Array.from({ length: 20 }, () => makeLayerId());
		}, MOD);
		expect(new Set(ids).size).toBe(20);
		for (const id of ids) expect(id).toMatch(/^layer-[0-9a-z]+-[0-9a-z]+$/);
	});
});

test.describe("constants", () => {
	test("AKP05 mask geometry matches the 2x5 key grid and the continuous touch strip", async ({ page }) => {
		const mask = await page.evaluate(async (mod) => {
			const m = await import(mod);
			return { mask: m.AKP05_MASK, max: m.MAX_LAYERS, handles: m.RESIZE_HANDLES.map((h: { label: string; x: number; y: number }) => `${h.label}|${h.x},${h.y}`) };
		}, MOD);
		expect(mask.max).toBe(64);
		expect(mask.mask).toMatchObject({ width: 810, height: 470, keySize: 126, keyX: [0, 170, 340, 510, 680], keyY: [0, 170], touchStrip: { x: 0, y: 340, width: 810, height: 130 } });
		expect(mask.handles).toHaveLength(8);
		expect(new Set(mask.handles.map((h) => h.split("|")[1])).size).toBe(8);
	});
});
