import { expect, test } from "@playwright/test";

import {
	AKP05_MASK,
	MAX_LAYERS,
	PREVIEW_PADDING,
	PREVIEW_PANEL_HORIZONTAL_PADDING,
	PREVIEW_PANEL_VERTICAL_PADDING,
	RESIZE_HANDLES,
	decodeImage,
	drawComposedImage,
	getFittedImageSize,
	getTransformBounds,
	makeLayerId,
	partitionFiles,
	readFile,
	rotateVector,
	type ImageLayer,
} from "$lib/startupImage";

const g = globalThis as any;
const img = (w: number, h: number) => ({ naturalWidth: w, naturalHeight: h }) as unknown as HTMLImageElement;
const layer = (over: Partial<ImageLayer> = {}): ImageLayer => ({ id: "l", name: "n", image: "x", zoom: 1, offset_x: 0, offset_y: 0, rotation: 0, ...over });

test.describe("constants", () => {
	test("limits and preview paddings", () => {
		expect(MAX_LAYERS).toBe(64);
		expect(PREVIEW_PADDING).toBe(16);
		expect(PREVIEW_PANEL_HORIZONTAL_PADDING).toBe(32);
		expect(PREVIEW_PANEL_VERTICAL_PADDING).toBe(32);
	});

	test("AKP05_MASK matches the 810x470 AKP05E_552A geometry", () => {
		expect(AKP05_MASK.width).toBe(810);
		expect(AKP05_MASK.height).toBe(470);
		expect(AKP05_MASK.keySize).toBe(126);
		expect(AKP05_MASK.keyX).toHaveLength(5);
		expect(AKP05_MASK.keyY).toHaveLength(2);
		expect(AKP05_MASK.touchStrip).toEqual({ x: 0, y: 340, width: 810, height: 130 });
		// Keys fit inside the canvas and above the touch strip.
		expect(AKP05_MASK.keyX.at(-1)! + AKP05_MASK.keySize).toBeLessThanOrEqual(AKP05_MASK.width);
		expect(AKP05_MASK.keyY.at(-1)! + AKP05_MASK.keySize).toBeLessThanOrEqual(AKP05_MASK.touchStrip.y);
		expect(AKP05_MASK.touchStrip.y + AKP05_MASK.touchStrip.height).toBe(AKP05_MASK.height);
	});

	test("RESIZE_HANDLES cover 4 corners and 4 edges with unique labels", () => {
		expect(RESIZE_HANDLES).toHaveLength(8);
		expect(new Set(RESIZE_HANDLES.map((h) => h.label)).size).toBe(8);
		const corners = RESIZE_HANDLES.filter((h) => h.x != 0 && h.y != 0);
		const edges = RESIZE_HANDLES.filter((h) => h.x == 0 || h.y == 0);
		expect(corners).toHaveLength(4);
		expect(edges).toHaveLength(4);
		expect(new Set(RESIZE_HANDLES.map((h) => `${h.x},${h.y}`)).size).toBe(8);
	});
});

test.describe("getFittedImageSize", () => {
	test("covers the output (fill scale) for wide images", () => {
		expect(getFittedImageSize(img(200, 100), { width: 100, height: 100 }, 1)).toEqual({ width: 200, height: 100 });
	});

	test("covers the output for tall images", () => {
		expect(getFittedImageSize(img(100, 200), { width: 100, height: 100 }, 1)).toEqual({ width: 100, height: 200 });
	});

	test("scales up small images and applies zoom", () => {
		expect(getFittedImageSize(img(10, 10), { width: 100, height: 100 }, 1)).toEqual({ width: 100, height: 100 });
		expect(getFittedImageSize(img(10, 10), { width: 100, height: 100 }, 2)).toEqual({ width: 200, height: 200 });
	});

	test("zero image size yields non-finite dimensions", () => {
		const size = getFittedImageSize(img(0, 0), { width: 10, height: 10 }, 1);
		expect(Number.isFinite(size.width)).toBe(false);
	});
});

test.describe("getTransformBounds", () => {
	test("zero rect when there is no layer or it is not decoded", () => {
		const zero = { left: 0, top: 0, width: 0, height: 0 };
		expect(getTransformBounds(undefined, 100, 100)).toEqual(zero);
		expect(getTransformBounds(layer(), 100, 100)).toEqual(zero);
	});

	test("zero rect when the output has no size", () => {
		const l = layer({ decoded: img(10, 10) });
		expect(getTransformBounds(l, 0, 100)).toEqual({ left: 0, top: 0, width: 0, height: 0 });
		expect(getTransformBounds(l, 100, 0)).toEqual({ left: 0, top: 0, width: 0, height: 0 });
	});

	test("a square image filling a square output is the unit rect", () => {
		expect(getTransformBounds(layer({ decoded: img(10, 10) }), 100, 100)).toEqual({ left: 0, top: 0, width: 1, height: 1 });
	});

	test("wide image is centered and overflows horizontally", () => {
		const bounds = getTransformBounds(layer({ decoded: img(200, 100) }), 100, 100);
		expect(bounds).toEqual({ left: -0.5, top: 0, width: 2, height: 1 });
	});

	test("offsets shift the bounds in normalized units, zoom enlarges around the center", () => {
		const shifted = getTransformBounds(layer({ decoded: img(10, 10), offset_x: 10, offset_y: -20 }), 100, 100);
		expect(shifted.left).toBeCloseTo(0.1);
		expect(shifted.top).toBeCloseTo(-0.2);
		const zoomed = getTransformBounds(layer({ decoded: img(10, 10), zoom: 2 }), 100, 100);
		expect(zoomed).toEqual({ left: -0.5, top: -0.5, width: 2, height: 2 });
	});
});

test.describe("rotateVector", () => {
	test("0 degrees is identity", () => {
		expect(rotateVector(3, 4, 0)).toEqual({ x: 3, y: 4 });
	});

	test("90 degrees maps +x to +y", () => {
		const r = rotateVector(1, 0, 90);
		expect(r.x).toBeCloseTo(0);
		expect(r.y).toBeCloseTo(1);
	});

	test("180 degrees negates; 360 is identity", () => {
		const half = rotateVector(2, 3, 180);
		expect(half.x).toBeCloseTo(-2);
		expect(half.y).toBeCloseTo(-3);
		const full = rotateVector(2, 3, 360);
		expect(full.x).toBeCloseTo(2);
		expect(full.y).toBeCloseTo(3);
	});

	test("negative angles rotate the other way and preserve length", () => {
		const r = rotateVector(1, 0, -90);
		expect(r.x).toBeCloseTo(0);
		expect(r.y).toBeCloseTo(-1);
		const s = rotateVector(3, 4, 37);
		expect(Math.hypot(s.x, s.y)).toBeCloseTo(5);
	});
});

test.describe("makeLayerId", () => {
	test("returns unique non-empty ids", () => {
		const ids = new Set(Array.from({ length: 50 }, () => makeLayerId()));
		expect(ids.size).toBe(50);
		for (const id of ids) expect(id.length).toBeGreaterThan(0);
	});

	test("falls back to a layer- prefixed id when crypto.randomUUID is unavailable", () => {
		const descriptor = Object.getOwnPropertyDescriptor(globalThis, "crypto");
		Object.defineProperty(globalThis, "crypto", { value: undefined, configurable: true });
		try {
			expect(makeLayerId()).toMatch(/^layer-[a-z0-9]+-[a-z0-9]*$/);
		} finally {
			if (descriptor) Object.defineProperty(globalThis, "crypto", descriptor);
		}
	});
});

test.describe("partitionFiles", () => {
	const file = (name: string, type = "", size = 10) => ({ name, type, size }) as unknown as File;

	test("accepts allowed extensions (case-insensitive) with matching or empty MIME type", () => {
		const { validFiles, rejected } = partitionFiles([file("a.PNG", "image/png"), file("b.jpg"), file("c.jpeg", "image/jpeg"), file("d.bmp", "image/x-ms-bmp"), file("e.svg", "image/svg+xml")]);
		expect(rejected).toEqual([]);
		expect(validFiles.map((v) => v.extension)).toEqual(["png", "jpg", "jpeg", "bmp", "svg"]);
	});

	test("rejects unsupported extensions and mismatching MIME types", () => {
		const { validFiles, rejected } = partitionFiles([file("a.gif", "image/gif"), file("noext"), file("b.png", "text/plain"), file("c.webp")]);
		expect(validFiles).toEqual([]);
		expect(rejected).toEqual(["a.gif has an unsupported file type", "noext has an unsupported file type", "b.png has an unsupported file type", "c.webp has an unsupported file type"]);
	});

	test("rejects files over 10 MB but accepts exactly 10 MB", () => {
		const limit = 10 * 1024 * 1024;
		const { validFiles, rejected } = partitionFiles([file("ok.png", "image/png", limit), file("big.png", "image/png", limit + 1)]);
		expect(validFiles.map((v) => v.file.name)).toEqual(["ok.png"]);
		expect(rejected).toEqual(["big.png is larger than 10 MB"]);
	});

	test("empty input", () => {
		expect(partitionFiles([])).toEqual({ validFiles: [], rejected: [] });
	});
});

test.describe("decodeImage (stubbed Image)", () => {
	let savedImage: unknown;
	test.beforeEach(() => (savedImage = g.Image));
	test.afterEach(() => (g.Image = savedImage));

	test("resolves with the image on load", async () => {
		g.Image = class {
			onload?: () => void;
			set src(_v: string) {
				queueMicrotask(() => this.onload?.());
			}
		};
		const image = await decodeImage("data:x");
		expect(image).toBeTruthy();
	});

	test("rejects with a friendly error on failure", async () => {
		g.Image = class {
			onerror?: () => void;
			set src(_v: string) {
				queueMicrotask(() => this.onerror?.());
			}
		};
		await expect(decodeImage("bad")).rejects.toThrow("The image could not be decoded");
	});
});

test.describe("readFile (stubbed FileReader, raster paths)", () => {
	let savedReader: unknown;
	test.beforeEach(() => (savedReader = g.FileReader));
	test.afterEach(() => (g.FileReader = savedReader));

	function stubReader(result: unknown, mode: "load" | "error" = "load") {
		const calls: string[] = [];
		g.FileReader = class {
			result = result;
			onload?: () => void;
			onerror?: () => void;
			readAsDataURL() {
				calls.push("dataurl");
				queueMicrotask(() => (mode == "load" ? this.onload?.() : this.onerror?.()));
			}
			readAsText() {
				calls.push("text");
				queueMicrotask(() => (mode == "load" ? this.onload?.() : this.onerror?.()));
			}
		};
		return calls;
	}
	const file = { name: "pic.x" } as File;

	test("png: rewrites the data URL MIME to image/png and reads as data URL", async () => {
		const calls = stubReader("data:application/octet-stream;base64,AAAA");
		expect(await readFile(file, "png")).toBe("data:image/png;base64,AAAA");
		expect(calls).toEqual(["dataurl"]);
	});

	test("bmp keeps image/bmp; jpg and jpeg normalise to image/jpeg", async () => {
		stubReader("data:foo/bar;base64,AAAA");
		expect(await readFile(file, "bmp")).toBe("data:image/bmp;base64,AAAA");
		expect(await readFile(file, "jpg")).toBe("data:image/jpeg;base64,AAAA");
		expect(await readFile(file, "jpeg")).toBe("data:image/jpeg;base64,AAAA");
	});

	test("svg is read as text", async () => {
		const calls = stubReader("<svg/>");
		await readFile(file, "svg").catch(() => undefined); // sanitising needs a DOM (browser-context)
		expect(calls).toEqual(["text"]);
	});

	test("rejects when the reader errors", async () => {
		stubReader(null, "error");
		await expect(readFile({ name: "broken.png" } as File, "png")).rejects.toThrow("broken.png could not be read");
	});

	test("rejects when the reader result is not a string", async () => {
		stubReader(new ArrayBuffer(1));
		await expect(readFile({ name: "odd.png" } as File, "png")).rejects.toThrow("odd.png could not be read");
	});
});

test.describe("drawComposedImage (fake canvas)", () => {
	function fakeCanvas() {
		const ops: unknown[][] = [];
		const context = {
			fillStyle: "",
			imageSmoothingEnabled: false,
			imageSmoothingQuality: "",
			fillRect: (...a: unknown[]) => ops.push(["fillRect", ...a]),
			save: () => ops.push(["save"]),
			restore: () => ops.push(["restore"]),
			translate: (...a: unknown[]) => ops.push(["translate", ...a]),
			rotate: (...a: unknown[]) => ops.push(["rotate", ...a]),
			drawImage: (...a: unknown[]) => ops.push(["drawImage", ...a.slice(1)]),
		};
		const canvas = { width: 0, height: 0, getContext: () => context } as unknown as HTMLCanvasElement;
		return { canvas, ops, context };
	}

	test("does nothing for a zero-sized output", () => {
		const { canvas, ops } = fakeCanvas();
		drawComposedImage(canvas, [], { width: 0, height: 100 });
		expect(ops).toHaveLength(0);
		expect(canvas.width).toBe(0);
	});

	test("resizes the canvas, fills black and skips undecoded layers", () => {
		const { canvas, ops, context } = fakeCanvas();
		drawComposedImage(canvas, [layer()], { width: 810, height: 470 });
		expect(canvas.width).toBe(810);
		expect(canvas.height).toBe(470);
		expect(context.fillStyle).toBe("#000000");
		expect(context.imageSmoothingEnabled).toBe(true);
		expect(context.imageSmoothingQuality).toBe("high");
		expect(ops).toEqual([["fillRect", 0, 0, 810, 470]]);
	});

	test("draws decoded layers centered with offset and rotation, in order", () => {
		const { canvas, ops } = fakeCanvas();
		drawComposedImage(canvas, [layer({ decoded: img(100, 100), offset_x: 5, offset_y: 7, rotation: 90 })], { width: 100, height: 100 });
		expect(ops.slice(1)).toEqual([["save"], ["translate", 55, 57], ["rotate", Math.PI / 2], ["drawImage", -50, -50, 100, 100], ["restore"]]);
	});

	test("returns silently when no 2d context is available", () => {
		const canvas = { width: 0, height: 0, getContext: () => null } as unknown as HTMLCanvasElement;
		expect(() => drawComposedImage(canvas, [], { width: 10, height: 10 })).not.toThrow();
		expect(canvas.width).toBe(10);
	});
});
