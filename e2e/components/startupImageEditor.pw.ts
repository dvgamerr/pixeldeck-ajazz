import type { Page } from "@playwright/test";

import { withHelpers } from "../lib/support";
import { AKP05_ID, akp05Device, openApp, test, expect, type TauriMock } from "../fixtures/tauriMock";

const STARTUP = { width: 810, height: 470 };
const withStartup = (extra: Record<string, unknown> = {}) => ({ [AKP05_ID]: { ...akp05Device(), startup_image: STARTUP, ...extra } });

const SVG_OK = '<svg xmlns="http://www.w3.org/2000/svg" width="81" height="47"><rect width="81" height="47" fill="#00ff00"/></svg>';

/** PNG bytes whose aspect ratio equals the 810x470 output, so the layer exactly covers the canvas at zoom 1. */
const png = async (page: Page, colour = "#ff0000", width = 81, height = 47) => {
	const url: string = await page.evaluate(([c, w, h]) => (window as any).__t.solid(c, w, h), [colour, width, height] as const);
	return Buffer.from(url.split(",")[1]!, "base64");
};
const file = async (page: Page, name: string, colour?: string) => ({ name, mimeType: "image/png", buffer: await png(page, colour) });

const openEditor = async (page: Page) => {
	await page.getByTestId("settings-open").click();
	await page.getByTestId("settings-tab-startup-image").click();
	await expect(page.getByTestId("startup-image-editor")).toBeVisible();
};

const status = (page: Page) => page.getByTestId("startup-status");
const layers = (page: Page) => page.getByTestId("startup-layer");
const names = (page: Page) => layers(page).locator("span.truncate").allTextContents();
const addFiles = async (page: Page, files: { name: string; mimeType: string; buffer: Buffer }[]) => page.getByTestId("startup-file-input").setInputFiles(files);

/** The project sent by the last Apply. */
const apply = async (page: Page, tauri: TauriMock) => {
	const before = (await tauri.calls("save_startup_image_project")).length;
	await page.getByTestId("startup-apply").click();
	await expect.poll(async () => (await tauri.calls("save_startup_image_project")).length).toBe(before + 1);
	return (await tauri.calls("save_startup_image_project")).at(-1)!.args as { device: string; project: { layers: any[] } };
};

const centreOf = async (page: Page, testId: string) => {
	const box = (await page.getByTestId(testId).boundingBox())!;
	return { x: box.x + box.width / 2, y: box.y + box.height / 2, width: box.width, height: box.height, left: box.x, top: box.y };
};

test.describe("DeviceStartupImage editor", () => {
	test.use({ options: { devices: withStartup() } });

	test.beforeEach(async ({ page }) => {
		await withHelpers(page);
		await openApp(page);
		await openEditor(page);
	});

	test.describe("initial state", () => {
		test("loads the saved project of the selected device and starts empty", async ({ page, tauri }) => {
			await expect(status(page)).toHaveAttribute("data-status", "empty");
			await expect(status(page)).toHaveText("No images");
			const calls = await tauri.calls("get_startup_image_project");
			expect(calls.at(-1)!.args).toEqual({ device: AKP05_ID });
			await expect(page.getByTestId("startup-layer-list")).toContainText("No images added");
			await expect(page.getByTestId("startup-layer-list")).toContainText("PNG, JPG, JPEG, BMP, or SVG");
			await expect(page.getByText("The black frame represents the device canvas.")).toBeVisible();
		});

		test("Apply and Reset selected are disabled without a layer, Add is enabled", async ({ page }) => {
			await expect(page.getByTestId("startup-apply")).toBeDisabled();
			await expect(page.getByTestId("startup-reset")).toBeDisabled();
			await expect(page.getByTestId("startup-add")).toBeEnabled();
		});

		test("the preview canvas has the device's startup resolution and is opaque black", async ({ page }) => {
			const canvas = page.getByTestId("startup-preview-canvas");
			expect(await canvas.evaluate((node) => [(node as HTMLCanvasElement).width, (node as HTMLCanvasElement).height])).toEqual([810, 470]);
			await expect.poll(() => canvas.evaluate((node) => Array.from((node as HTMLCanvasElement).getContext("2d")!.getImageData(400, 200, 1, 1).data))).toEqual([0, 0, 0, 255]);
		});

		test("draws the AKP05E_552A key and touch-strip mask over the preview", async ({ page }) => {
			const mask = page.getByTestId("startup-mask");
			await expect(mask).toBeVisible();
			await expect(mask).toHaveAttribute("viewBox", "0 0 810 470");
			// 2x5 key outlines plus the continuous 810x130 touch strip.
			await expect(mask.locator("g rect")).toHaveCount(11);
			const strip = mask.locator("g rect").last();
			await expect(strip).toHaveAttribute("y", "340");
			await expect(strip).toHaveAttribute("height", "130");
			await expect(mask.locator("mask rect[fill='black']")).toHaveCount(11);
		});

		test("the preview keeps the device aspect ratio", async ({ page }) => {
			const box = await centreOf(page, "startup-viewport");
			expect(box.width / box.height).toBeCloseTo(810 / 470, 1);
		});
	});

	test.describe("adding images", () => {
		test("the Add button opens a multi-file chooser restricted to images", async ({ page }) => {
			const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("startup-add").click()]);
			expect(chooser.isMultiple()).toBe(true);
			await expect(page.getByTestId("startup-file-input")).toHaveAttribute("accept", ".png,.jpg,.jpeg,.bmp,.svg,image/png,image/jpeg,image/bmp,image/svg+xml");
		});

		test("the empty-list call to action opens the chooser as well", async ({ page }) => {
			const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("startup-layer-list").getByRole("button", { name: "Add images" }).click()]);
			expect(chooser).toBeTruthy();
		});

		test("a PNG becomes a selected layer and marks the project as unsaved", async ({ page }) => {
			await addFiles(page, [await file(page, "logo.png")]);
			await expect(layers(page)).toHaveCount(1);
			await expect(layers(page).first()).toContainText("logo.png");
			await expect(layers(page).first()).toContainText("Layer 1");
			await expect(layers(page).first()).toHaveClass(/border-primary/);
			await expect(page.getByTestId("startup-layer-list").locator(".badge").first()).toHaveText("1");
			await expect(status(page)).toHaveAttribute("data-status", "dirty");
			await expect(page.getByTestId("startup-apply")).toBeEnabled();
			await expect(page.getByTestId("startup-reset")).toBeEnabled();
			await expect(page.getByTestId("startup-error")).toHaveCount(0);
		});

		test("the layer is composed into the preview", async ({ page }) => {
			await addFiles(page, [await file(page, "red.png", "#ff0000")]);
			const pixel = () => page.getByTestId("startup-preview-canvas").evaluate((node) => Array.from((node as HTMLCanvasElement).getContext("2d")!.getImageData(400, 200, 1, 1).data));
			await expect.poll(pixel).toEqual([255, 0, 0, 255]);
		});

		test("several files are added in order and the last one is selected", async ({ page }) => {
			await addFiles(page, [await file(page, "a.png"), await file(page, "b.png", "#00ff00"), await file(page, "c.png", "#0000ff")]);
			// The list shows the top layer first.
			await expect.poll(() => names(page)).toEqual(["c.png", "b.png", "a.png"]);
			await expect(layers(page).first()).toHaveClass(/border-primary/);
			await expect(layers(page).first()).toContainText("Layer 3");
			await expect(layers(page).last()).toContainText("Layer 1");
		});

		test("later layers are drawn above earlier ones", async ({ page }) => {
			await addFiles(page, [await file(page, "a.png", "#ff0000"), await file(page, "b.png", "#0000ff")]);
			const pixel = () => page.getByTestId("startup-preview-canvas").evaluate((node) => Array.from((node as HTMLCanvasElement).getContext("2d")!.getImageData(400, 200, 1, 1).data));
			await expect.poll(pixel).toEqual([0, 0, 255, 255]);
		});

		test("an SVG is sanitised into a base64 SVG layer", async ({ page, tauri }) => {
			await addFiles(page, [{ name: "vector.svg", mimeType: "image/svg+xml", buffer: Buffer.from(SVG_OK) }]);
			await expect(layers(page)).toHaveCount(1);
			const project = await apply(page, tauri);
			expect(project.project.layers[0].image).toMatch(/^data:image\/svg\+xml;base64,/);
		});

		test("JPEG and BMP extensions are accepted", async ({ page }) => {
			const base = await png(page);
			await addFiles(page, [
				{ name: "photo.jpg", mimeType: "image/jpeg", buffer: base },
				{ name: "legacy.bmp", mimeType: "image/bmp", buffer: base },
			]);
			// The bytes are PNG, but the browser sniffs the real format when decoding.
			await expect(layers(page)).toHaveCount(2);
		});
	});

	test.describe("file validation", () => {
		test("rejects unsupported file types and shows why", async ({ page }) => {
			await addFiles(page, [{ name: "anim.gif", mimeType: "image/gif", buffer: Buffer.from("GIF89a") }]);
			await expect(page.getByTestId("startup-error")).toHaveText("anim.gif has an unsupported file type");
			await expect(layers(page)).toHaveCount(0);
			await expect(status(page)).toHaveAttribute("data-status", "empty");
		});

		test("rejects a file whose mime type contradicts its extension", async ({ page }) => {
			await addFiles(page, [{ name: "fake.png", mimeType: "text/html", buffer: Buffer.from("<html>") }]);
			await expect(page.getByTestId("startup-error")).toHaveText("fake.png has an unsupported file type");
		});

		test("rejects files larger than 10 MB", async ({ page }) => {
			await addFiles(page, [{ name: "huge.png", mimeType: "image/png", buffer: Buffer.alloc(10 * 1024 * 1024 + 1) }]);
			await expect(page.getByTestId("startup-error")).toHaveText("huge.png is larger than 10 MB");
			await expect(layers(page)).toHaveCount(0);
		});

		test("valid files are still added next to rejected ones, and every rejection is reported", async ({ page }) => {
			await addFiles(page, [
				await file(page, "good.png"),
				{ name: "bad.gif", mimeType: "image/gif", buffer: Buffer.from("GIF89a") },
				{ name: "bad.txt", mimeType: "text/plain", buffer: Buffer.from("hi") },
			]);
			await expect(layers(page)).toHaveCount(1);
			await expect(page.getByTestId("startup-error")).toHaveText("bad.gif has an unsupported file type. bad.txt has an unsupported file type");
		});

		test("a corrupt image is reported with the decoder error", async ({ page }) => {
			await addFiles(page, [{ name: "corrupt.png", mimeType: "image/png", buffer: Buffer.from("this is not a png") }]);
			await expect(page.getByTestId("startup-error")).toContainText("The image could not be decoded");
			await expect(layers(page)).toHaveCount(0);
		});

		test("an invalid SVG is reported by name", async ({ page }) => {
			await addFiles(page, [{ name: "broken.svg", mimeType: "image/svg+xml", buffer: Buffer.from("<html><body>nope</body></html>") }]);
			await expect(page.getByTestId("startup-error")).toContainText("broken.svg is not a valid SVG image");
			await expect(layers(page)).toHaveCount(0);
		});

		test("scripts inside an SVG never reach the stored layer", async ({ page, tauri }) => {
			const dirty = '<svg xmlns="http://www.w3.org/2000/svg" width="81" height="47" onload="alert(1)"><script>alert(2)</script><rect width="81" height="47" fill="#00ff00"/></svg>';
			await addFiles(page, [{ name: "dirty.svg", mimeType: "image/svg+xml", buffer: Buffer.from(dirty) }]);
			await expect(layers(page)).toHaveCount(1);
			const project = await apply(page, tauri);
			const stored = Buffer.from(project.project.layers[0].image.split(",")[1], "base64").toString("utf8");
			expect(stored).not.toContain("script");
			expect(stored).not.toContain("onload");
			expect(stored).toContain("<rect");
		});

		test("the next successful add clears an earlier error", async ({ page }) => {
			await addFiles(page, [{ name: "anim.gif", mimeType: "image/gif", buffer: Buffer.from("GIF89a") }]);
			await expect(page.getByTestId("startup-error")).toBeVisible();
			await addFiles(page, [await file(page, "ok.png")]);
			await expect(page.getByTestId("startup-error")).toHaveCount(0);
		});

		test("more than 64 images are refused as a whole", async ({ page }) => {
			const one = await file(page, "x.png");
			await addFiles(
				page,
				Array.from({ length: 65 }, (_, i) => ({ ...one, name: `l${i}.png` })),
			);
			await expect(page.getByTestId("startup-error")).toHaveText("A startup image can contain at most 64 images.");
			await expect(layers(page)).toHaveCount(0);
		});

		test("64 images are accepted and then Add is disabled", async ({ page }) => {
			const one = await file(page, "x.png");
			await addFiles(
				page,
				Array.from({ length: 64 }, (_, i) => ({ ...one, name: `l${i}.png` })),
			);
			await expect(layers(page)).toHaveCount(64, { timeout: 30_000 });
			await expect(page.getByTestId("startup-add")).toBeDisabled();
			await expect(page.getByTestId("startup-error")).toHaveCount(0);
		});
	});

	test.describe("layer list", () => {
		test.beforeEach(async ({ page }) => {
			await addFiles(page, [await file(page, "a.png", "#ff0000"), await file(page, "b.png", "#00ff00"), await file(page, "c.png", "#0000ff")]);
			await expect(layers(page)).toHaveCount(3);
		});

		test("clicking a layer selects it", async ({ page }) => {
			const a = layers(page).filter({ hasText: "a.png" });
			await a.getByTestId("startup-layer-select").click();
			await expect(a).toHaveClass(/border-primary/);
			await expect(layers(page).filter({ hasText: "c.png" })).not.toHaveClass(/border-primary/);
		});

		test("removing the selected layer selects its neighbour", async ({ page }) => {
			await layers(page).filter({ hasText: "b.png" }).getByTestId("startup-layer-select").click();
			await layers(page).filter({ hasText: "b.png" }).getByTestId("startup-layer-remove").click();
			await expect.poll(() => names(page)).toEqual(["c.png", "a.png"]);
			// Index 1 (b) was removed; the layer that took its place (c) becomes active.
			await expect(layers(page).filter({ hasText: "c.png" })).toHaveClass(/border-primary/);
		});

		test("removing a layer that is not selected keeps the selection", async ({ page }) => {
			await layers(page).filter({ hasText: "a.png" }).getByTestId("startup-layer-remove").click();
			await expect.poll(() => names(page)).toEqual(["c.png", "b.png"]);
			await expect(layers(page).filter({ hasText: "c.png" })).toHaveClass(/border-primary/);
		});

		test("removing every layer returns to the empty state", async ({ page }) => {
			for (let i = 0; i < 3; i++) await layers(page).first().getByTestId("startup-layer-remove").click();
			await expect(layers(page)).toHaveCount(0);
			await expect(page.getByTestId("startup-layer-list")).toContainText("No images added");
			await expect(status(page)).toHaveAttribute("data-status", "empty");
			await expect(page.getByTestId("startup-apply")).toBeDisabled();
			await expect(page.getByTestId("startup-reset")).toBeDisabled();
		});

		test("move up and down reorder the stack and disable at the ends", async ({ page }) => {
			const row = (name: string) => layers(page).filter({ hasText: name });
			await expect(row("c.png").getByRole("button", { name: "Move c.png up" })).toBeDisabled();
			await expect(row("a.png").getByRole("button", { name: "Move a.png down" })).toBeDisabled();
			await row("a.png").getByRole("button", { name: "Move a.png up" }).click();
			await expect.poll(() => names(page)).toEqual(["c.png", "a.png", "b.png"]);
			await row("c.png").getByRole("button", { name: "Move c.png down" }).click();
			await expect.poll(() => names(page)).toEqual(["a.png", "c.png", "b.png"]);
			await expect(row("a.png")).toContainText("Layer 3");
		});

		test("reordering is reflected in the composed preview and in the saved project order", async ({ page, tauri }) => {
			const pixel = () => page.getByTestId("startup-preview-canvas").evaluate((node) => Array.from((node as HTMLCanvasElement).getContext("2d")!.getImageData(400, 200, 1, 1).data));
			await expect.poll(pixel).toEqual([0, 0, 255, 255]);
			await layers(page).filter({ hasText: "c.png" }).getByRole("button", { name: "Move c.png down" }).click();
			await expect.poll(pixel).toEqual([0, 255, 0, 255]);
			const saved = await apply(page, tauri);
			expect(saved.project.layers.map((layer) => layer.name)).toEqual(["a.png", "c.png", "b.png"]);
		});

		test("moving or removing marks the project as unsaved again after an apply", async ({ page, tauri }) => {
			await apply(page, tauri);
			await expect(status(page)).toHaveAttribute("data-status", "applied");
			await layers(page)
				.first()
				.getByRole("button", { name: /Move c.png down/ })
				.click();
			await expect(status(page)).toHaveAttribute("data-status", "dirty");
		});
	});

	test.describe("placement", () => {
		test.beforeEach(async ({ page }) => {
			await addFiles(page, [await file(page, "cover.png", "#ff0000")]);
			await expect(layers(page)).toHaveCount(1);
		});

		test("dragging the preview moves the selected layer in output pixels", async ({ page, tauri }) => {
			const view = await centreOf(page, "startup-viewport");
			await page.mouse.move(view.x, view.y);
			await page.mouse.down();
			await page.mouse.move(view.x + 40, view.y + 20, { steps: 5 });
			await page.mouse.up();
			const { project } = await apply(page, tauri);
			expect(project.layers[0].offset_x).toBeCloseTo((40 * 810) / view.width, 0);
			expect(project.layers[0].offset_y).toBeCloseTo((20 * 470) / view.height, 0);
			expect(project.layers[0].zoom).toBe(1);
			expect(project.layers[0].rotation).toBe(0);
		});

		test("moving the pointer after releasing does not drag any more", async ({ page, tauri }) => {
			const view = await centreOf(page, "startup-viewport");
			await page.mouse.move(view.x, view.y);
			await page.mouse.down();
			await page.mouse.move(view.x + 20, view.y, { steps: 3 });
			await page.mouse.up();
			await page.mouse.move(view.x + 120, view.y + 50, { steps: 3 });
			const { project } = await apply(page, tauri);
			expect(project.layers[0].offset_x).toBeCloseTo((20 * 810) / view.width, 0);
			expect(project.layers[0].offset_y).toBeCloseTo(0, 0);
		});

		test("Reset selected restores zoom, offset and rotation", async ({ page, tauri }) => {
			const view = await centreOf(page, "startup-viewport");
			await page.mouse.move(view.x, view.y);
			await page.mouse.down();
			await page.mouse.move(view.x + 60, view.y + 30, { steps: 4 });
			await page.mouse.up();
			await expect(status(page)).toHaveAttribute("data-status", "dirty");
			await page.getByTestId("startup-reset").click();
			const { project } = await apply(page, tauri);
			expect(project.layers[0]).toMatchObject({ zoom: 1, offset_x: 0, offset_y: 0, rotation: 0 });
		});

		test("the selection box has eight resize handles and a rotate handle", async ({ page }) => {
			for (const label of ["top left", "top right", "bottom left", "bottom right", "top", "bottom", "left", "right"]) {
				await expect(page.getByRole("button", { name: `Resize from ${label}`, exact: true })).toBeVisible();
			}
			await expect(page.getByRole("button", { name: "Rotate selected image" })).toBeAttached();
		});

		test("dragging the bottom-right handle towards the centre scales around the opposite corner", async ({ page, tauri }) => {
			const view = await centreOf(page, "startup-viewport");
			const handle = (await page.getByRole("button", { name: "Resize from bottom right", exact: true }).boundingBox())!;
			await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
			await page.mouse.down();
			await page.mouse.move(view.x, view.y, { steps: 6 });
			await page.mouse.up();
			const { project } = await apply(page, tauri);
			expect(project.layers[0].zoom).toBeCloseTo(0.5, 1);
			// The top-left corner stays put: the centre shifts up-left by a quarter of the canvas.
			expect(project.layers[0].offset_x).toBeCloseTo(-202.5, -1);
			expect(project.layers[0].offset_y).toBeCloseTo(-117.5, -1);
		});

		test("zoom is clamped at 0.25 when shrinking and grows past 1 when dragging outwards", async ({ page, tauri }) => {
			const view = await centreOf(page, "startup-viewport");
			const handle = (await page.getByRole("button", { name: "Resize from bottom right", exact: true }).boundingBox())!;
			await page.mouse.move(handle.x + handle.width / 2, handle.y + handle.height / 2);
			await page.mouse.down();
			await page.mouse.move(view.left, view.top, { steps: 4 });
			await page.mouse.up();
			expect((await apply(page, tauri)).project.layers[0].zoom).toBe(0.25);

			await page.getByTestId("startup-reset").click();
			const handle2 = (await page.getByRole("button", { name: "Resize from top left", exact: true }).boundingBox())!;
			await page.mouse.move(handle2.x + handle2.width / 2, handle2.y + handle2.height / 2);
			await page.mouse.down();
			await page.mouse.move(view.left - 200, view.top - 100, { steps: 4 });
			await page.mouse.up();
			const grown = (await apply(page, tauri)).project.layers[0].zoom;
			expect(grown).toBeGreaterThan(1);
			expect(grown).toBeLessThanOrEqual(3);
		});

		test("the rotate handle turns the layer around its centre", async ({ page, tauri }) => {
			// Shrink the layer to the top-left quadrant first so the rotate handle sits inside the visible preview.
			const view = await centreOf(page, "startup-viewport");
			const corner = (await page.getByRole("button", { name: "Resize from bottom right", exact: true }).boundingBox())!;
			await page.mouse.move(corner.x + corner.width / 2, corner.y + corner.height / 2);
			await page.mouse.down();
			await page.mouse.move(view.x, view.y, { steps: 6 });
			await page.mouse.up();

			const rotate = (await page.getByRole("button", { name: "Rotate selected image" }).boundingBox())!;
			const handleX = rotate.x + rotate.width / 2;
			const handleY = rotate.y + rotate.height / 2;
			const layerCentreX = view.left + view.width * 0.25;
			const layerCentreY = view.top + view.height * 0.25;
			// Handle is directly below the centre (angle +90deg). Move it to the right of the centre (angle 0deg): -90deg rotation.
			const radius = Math.hypot(handleX - layerCentreX, handleY - layerCentreY);
			await page.mouse.move(handleX, handleY);
			await page.mouse.down();
			await page.mouse.move(layerCentreX + radius, layerCentreY, { steps: 8 });
			await page.mouse.up();
			const { project } = await apply(page, tauri);
			expect(project.layers[0].rotation).toBeCloseTo(-90, -1);
		});
	});

	test.describe("apply", () => {
		test.beforeEach(async ({ page }) => {
			await addFiles(page, [await file(page, "cover.png", "#ff0000")]);
			await expect(layers(page)).toHaveCount(1);
		});

		test("saves the project and then sends the composed JPEG to the device", async ({ page, tauri }) => {
			await page.getByTestId("startup-apply").click();
			await expect(status(page)).toHaveAttribute("data-status", "applied");
			await expect(status(page)).toHaveText("Applied & saved");

			const saves = await tauri.calls("save_startup_image_project");
			const sets = await tauri.calls("set_startup_image");
			expect(saves).toHaveLength(1);
			expect(sets).toHaveLength(1);
			// Order: save first, then set.
			const order = (await tauri.calls("")).map((call) => call.cmd).filter((cmd) => cmd == "save_startup_image_project" || cmd == "set_startup_image");
			expect(order).toEqual(["save_startup_image_project", "set_startup_image"]);

			expect(saves[0]!.args.device).toBe(AKP05_ID);
			expect(Object.keys(saves[0]!.args.project)).toEqual(["layers"]);
			expect(Object.keys(saves[0]!.args.project.layers[0]).sort()).toEqual(["id", "image", "name", "offset_x", "offset_y", "rotation", "zoom"]);
			expect(saves[0]!.args.project.layers[0]).toMatchObject({ name: "cover.png", zoom: 1, offset_x: 0, offset_y: 0, rotation: 0 });
			expect(saves[0]!.args.project.layers[0].image).toMatch(/^data:image\/png;base64,/);

			expect(sets[0]!.args.device).toBe(AKP05_ID);
			const image = sets[0]!.args.image as string;
			expect(image).toMatch(/^data:image\/jpeg;base64,/);
			const decoded = await page.evaluate(async (url) => {
				const t = (window as any).__t;
				const canvas = await t.toCanvas(url);
				return { size: [canvas.width, canvas.height], centre: t.px(canvas, 405, 235) };
			}, image);
			expect(decoded.size).toEqual([810, 470]);
			expect(decoded.centre[0]).toBeGreaterThan(240);
			expect(decoded.centre[1]).toBeLessThan(20);
			expect(decoded.centre[2]).toBeLessThan(20);
		});

		test("shows a busy state and blocks other controls while applying", async ({ page, tauri }) => {
			await tauri.hold("set_startup_image");
			await page.getByTestId("startup-apply").click();
			await expect(page.getByTestId("startup-apply")).toContainText("Applying");
			await expect(page.getByTestId("startup-apply")).toBeDisabled();
			await expect(page.getByTestId("startup-reset")).toBeDisabled();
			await tauri.release("set_startup_image");
			await expect(status(page)).toHaveAttribute("data-status", "applied");
			await expect(page.getByTestId("startup-apply")).toHaveText("Apply");
		});

		test("a second click while applying does not start another apply", async ({ page, tauri }) => {
			await tauri.hold("set_startup_image");
			await page.getByTestId("startup-apply").click();
			await page.getByTestId("startup-apply").click({ force: true });
			await tauri.release("set_startup_image");
			await expect(status(page)).toHaveAttribute("data-status", "applied");
			expect(await tauri.calls("save_startup_image_project")).toHaveLength(1);
			expect(await tauri.calls("set_startup_image")).toHaveLength(1);
		});

		test("a failed save is reported and nothing is sent to the device", async ({ page, tauri }) => {
			await tauri.fail("save_startup_image_project", "disk full");
			await page.getByTestId("startup-apply").click();
			await expect(page.getByTestId("startup-error")).toHaveText("Unable to save startup image: disk full");
			expect(await tauri.calls("set_startup_image")).toHaveLength(0);
			await expect(status(page)).toHaveAttribute("data-status", "dirty");
			await expect(page.getByTestId("startup-apply")).toBeEnabled();
		});

		test("a failed device upload keeps the saved project and says so", async ({ page, tauri }) => {
			await tauri.fail("set_startup_image", "device unplugged");
			await page.getByTestId("startup-apply").click();
			await expect(page.getByTestId("startup-error")).toHaveText("Saved, but could not apply to the device: device unplugged");
			await expect(status(page)).toHaveAttribute("data-status", "saved");
			expect(await tauri.calls("save_startup_image_project")).toHaveLength(1);
			await expect(page.getByTestId("startup-apply")).toBeEnabled();
		});

		test("retrying after an error succeeds and clears the message", async ({ page, tauri }) => {
			await tauri.fail("set_startup_image", "device unplugged");
			await page.getByTestId("startup-apply").click();
			await expect(page.getByTestId("startup-error")).toBeVisible();
			await tauri.fail("set_startup_image", null);
			await page.getByTestId("startup-apply").click();
			await expect(page.getByTestId("startup-error")).toHaveCount(0);
			await expect(status(page)).toHaveAttribute("data-status", "applied");
		});

		test("the saved project is reloaded when settings are reopened", async ({ page }) => {
			await page.getByTestId("startup-apply").click();
			await expect(status(page)).toHaveAttribute("data-status", "applied");
			await page.getByTestId("settings-header-close").click();
			await openEditor(page);
			await expect(layers(page)).toHaveCount(1);
			await expect(layers(page).first()).toContainText("cover.png");
			await expect(status(page)).toHaveAttribute("data-status", "saved");
		});
	});
});

test.describe("DeviceStartupImage editor: loading", () => {
	test.use({ options: { devices: withStartup() } });

	test.beforeEach(async ({ page }) => {
		await withHelpers(page);
	});

	test("shows a loading state while the project is fetched and blocks input", async ({ page, tauri }) => {
		await openApp(page);
		await tauri.hold("get_startup_image_project");
		await page.getByTestId("settings-open").click();
		await page.getByTestId("settings-tab-startup-image").click();
		await expect(status(page)).toHaveAttribute("data-status", "loading");
		await expect(page.getByTestId("startup-add")).toBeDisabled();
		await expect(page.getByTestId("startup-apply")).toBeDisabled();
		await tauri.release("get_startup_image_project");
		await expect(status(page)).toHaveAttribute("data-status", "empty");
		await expect(page.getByTestId("startup-add")).toBeEnabled();
	});

	test("restores a saved project: layers in stack order, last layer selected, status saved", async ({ page, tauri }) => {
		await openApp(page);
		const red: string = await page.evaluate(() => (window as any).__t.solid("#ff0000", 81, 47));
		const blue: string = await page.evaluate(() => (window as any).__t.solid("#0000ff", 81, 47));
		await tauri.mockState(
			`(s) => { s.startupProjects["${AKP05_ID}"] = { layers: [
				{ id: "one", name: "red.png", image: ${JSON.stringify(red)}, zoom: 1, offset_x: 0, offset_y: 0, rotation: 0 },
				{ id: "two", name: "blue.png", image: ${JSON.stringify(blue)}, zoom: 0.5, offset_x: 100, offset_y: 0, rotation: 0 } ] }; }`,
		);
		await openEditor(page);
		await expect.poll(() => names(page)).toEqual(["blue.png", "red.png"]);
		await expect(layers(page).first()).toHaveClass(/border-primary/);
		await expect(status(page)).toHaveAttribute("data-status", "saved");
		const pixel = (x: number, y: number) =>
			page.getByTestId("startup-preview-canvas").evaluate((node, p) => Array.from((node as HTMLCanvasElement).getContext("2d")!.getImageData(p.x, p.y, 1, 1).data), { x, y });
		// Red covers the canvas, blue is half size and shifted 100px right of the centre.
		await expect.poll(() => pixel(5, 5)).toEqual([255, 0, 0, 255]);
		expect(await pixel(505, 235)).toEqual([0, 0, 255, 255]);
	});

	test("a failing load is reported", async ({ page, tauri }) => {
		await openApp(page);
		await tauri.fail("get_startup_image_project", "database locked");
		await openEditor(page);
		await expect(page.getByTestId("startup-error")).toHaveText("Unable to load saved startup image: database locked");
		await expect(status(page)).toHaveAttribute("data-status", "empty");
	});
});

test.describe("DeviceStartupImage editor: other devices", () => {
	test.use({ options: { devices: withStartup({ type: 1, startup_image: { width: 480, height: 272 } }) } });

	test("devices that are not an AKP05E_552A get no layout mask and use their own resolution", async ({ page }) => {
		await withHelpers(page);
		await openApp(page);
		await openEditor(page);
		await expect(page.getByTestId("startup-mask")).toHaveCount(0);
		const size = await page.getByTestId("startup-preview-canvas").evaluate((node) => [(node as HTMLCanvasElement).width, (node as HTMLCanvasElement).height]);
		expect(size).toEqual([480, 272]);
	});
});
