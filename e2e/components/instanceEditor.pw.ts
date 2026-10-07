import type { Page } from "@playwright/test";

import { withHelpers } from "../lib/support";
import { KEY_ACTION, openApp, test, expect, type TauriMock } from "../fixtures/tauriMock";

const GIF = "data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==";

const state = (overrides: Record<string, unknown> = {}) => ({
	...KEY_ACTION.states[0],
	image: "",
	name: "",
	text: "",
	show: false,
	colour: "#ffffff",
	alignment: "middle",
	family: "Arial",
	style: "Regular",
	size: 16,
	underline: false,
	...overrides,
});
const TWO_STATES = {
	...KEY_ACTION,
	uuid: "test.plugin.two",
	name: "Two States",
	states: [state({ image: "state-one.png", text: "one" }), state({ image: "state-two.png", text: "two", colour: "#00ff00" })],
};

const key = (page: Page) => page.getByTestId("device-key-2");
const editor = (page: Page) => page.getByTestId("instance-editor");

const openEditor = async (page: Page) => {
	await key(page).getByTestId("key-canvas").click({ button: "right" });
	await page.getByTestId("context-menu-edit").click();
	await expect(editor(page)).toBeVisible();
};

/** The instance the editor last reported to the backend. */
const lastSetState = async (tauri: TauriMock) => {
	const call = (await tauri.calls("set_state")).at(-1);
	return call?.args as { instance: any; state: number } | undefined;
};

const PNG_RED_2X1 = async (page: Page) => {
	const url = await page.evaluate(() => (window as any).__t.solid("#ff0000", 2, 1));
	return Buffer.from(url.split(",")[1], "base64");
};

test.describe("InstanceEditor", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 2, action: TWO_STATES }] } });

	test.beforeEach(async ({ page }) => {
		await withHelpers(page);
		await openApp(page);
		await openEditor(page);
	});

	test("is portalled into the device workspace as a modal dialog", async ({ page }) => {
		const info = await editor(page).evaluate((node) => ({ inWorkspace: node.parentElement?.getAttribute("data-testid"), dialog: node.querySelector('[role="dialog"]')?.getAttribute("aria-label") }));
		expect(info).toEqual({ inWorkspace: "device-workspace", dialog: "Edit key appearance" });
		await expect(editor(page).getByRole("heading", { name: "Customize state 1" })).toBeVisible();
	});

	test("reports the instance to the backend as soon as it opens", async ({ tauri }) => {
		await expect.poll(async () => (await lastSetState(tauri))?.instance.context).toBe("sd-TEST.Default.Keypad.2.0");
		expect((await lastSetState(tauri))!.state).toBe(0);
	});

	test.describe("closing", () => {
		test("Done closes", async ({ page }) => {
			await page.getByTestId("instance-editor-done").click();
			await expect(editor(page)).toHaveCount(0);
		});
		test("the close button closes", async ({ page }) => {
			await page.getByTestId("instance-editor-close").click();
			await expect(editor(page)).toHaveCount(0);
		});
		test("Escape closes", async ({ page }) => {
			await page.keyboard.press("Escape");
			await expect(editor(page)).toHaveCount(0);
		});
		test("clicking the backdrop closes", async ({ page }) => {
			await page.getByTestId("instance-editor-backdrop").click({ position: { x: 5, y: 5 } });
			await expect(editor(page)).toHaveCount(0);
		});
		test("clicking inside the dialog does not close it", async ({ page }) => {
			await editor(page).getByRole("heading", { name: "Typography" }).click();
			await expect(editor(page)).toBeVisible();
		});
		test("the edited values survive closing and reopening", async ({ page }) => {
			await page.getByTestId("instance-editor-text").fill("kept");
			await page.getByTestId("instance-editor-done").click();
			await openEditor(page);
			await expect(page.getByTestId("instance-editor-text")).toHaveValue("kept");
		});
	});

	test.describe("states", () => {
		test("the state picker has one entry per action state", async ({ page }) => {
			await expect(page.getByTestId("instance-editor-state").locator("option")).toHaveText(["State 1", "State 2"]);
		});

		test("switching state loads that state's values and retitles the dialog", async ({ page, tauri }) => {
			await expect(page.getByTestId("instance-editor-text")).toHaveValue("one");
			await page.getByTestId("instance-editor-state").selectOption("1");
			await expect(editor(page).getByRole("heading", { name: "Customize state 2" })).toBeVisible();
			await expect(page.getByTestId("instance-editor-text")).toHaveValue("two");
			await expect(page.getByTestId("instance-editor-colour")).toHaveValue("#00ff00");
			await expect(page.getByTestId("instance-editor-state-badge")).toHaveText("State 2");
			await expect.poll(async () => (await lastSetState(tauri))?.state).toBe(1);
		});

		test("edits apply to the selected state only", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-state").selectOption("1");
			await page.getByTestId("instance-editor-text").fill("second");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[1].text).toBe("second");
			expect((await lastSetState(tauri))!.instance.states[0].text).toBe("one");
		});

		test("the preview shows the state's own image", async ({ page }) => {
			await expect(page.getByTestId("instance-editor-preview")).toHaveAttribute("src", /state-one\.png$/);
			await page.getByTestId("instance-editor-state").selectOption("1");
			await expect(page.getByTestId("instance-editor-preview")).toHaveAttribute("src", /state-two\.png$/);
		});

		test("the bold/italic toggles follow the selected state", async ({ page }) => {
			await page.getByTestId("instance-editor-bold").click();
			await expect(page.getByTestId("instance-editor-bold")).toHaveAttribute("aria-pressed", "true");
			await page.getByTestId("instance-editor-state").selectOption("1");
			await expect(page.getByTestId("instance-editor-bold")).toHaveAttribute("aria-pressed", "false");
			await page.getByTestId("instance-editor-state").selectOption("0");
			await expect(page.getByTestId("instance-editor-bold")).toHaveAttribute("aria-pressed", "true");
		});
	});

	test.describe("label", () => {
		test("Show text toggles states[n].show", async ({ page, tauri }) => {
			const toggle = page.getByTestId("instance-editor-show-text");
			await expect(toggle).not.toBeChecked();
			await toggle.check();
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].show).toBe(true);
			await toggle.uncheck();
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].show).toBe(false);
		});

		test("the text area edits the label, including multiple lines", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-text").fill("line 1\nline 2");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].text).toBe("line 1\nline 2");
		});

		test("the label is drawn on the key only while Show text is on", async ({ page }) => {
			const canvasUrl = () =>
				key(page)
					.getByTestId("key-canvas")
					.evaluate((node) => (node as HTMLCanvasElement).toDataURL());
			await expect.poll(canvasUrl).toBeTruthy();
			await page.getByTestId("instance-editor-text").fill("Hello");
			const hidden = await canvasUrl();
			await page.getByTestId("instance-editor-show-text").check();
			await expect.poll(canvasUrl).not.toBe(hidden);
			const shown = await canvasUrl();
			await page.getByTestId("instance-editor-show-text").uncheck();
			await expect.poll(canvasUrl).not.toBe(shown);
		});
	});

	test.describe("typography", () => {
		test("bold, italic and bold+italic map onto the style string", async ({ page, tauri }) => {
			const bold = page.getByTestId("instance-editor-bold");
			const italic = page.getByTestId("instance-editor-italic");
			await expect(bold).toHaveAttribute("aria-pressed", "false");
			await bold.click();
			await expect(bold).toHaveAttribute("aria-pressed", "true");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].style).toBe("Bold");
			await italic.click();
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].style).toBe("Bold Italic");
			await bold.click();
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].style).toBe("Italic");
			await italic.click();
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].style).toBe("Regular");
			await expect(bold).toHaveAttribute("aria-pressed", "false");
			await expect(italic).toHaveAttribute("aria-pressed", "false");
		});

		test("underline toggles independently of the style", async ({ page, tauri }) => {
			const underline = page.getByTestId("instance-editor-underline");
			await underline.click();
			await expect(underline).toHaveAttribute("aria-pressed", "true");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].underline).toBe(true);
			expect((await lastSetState(tauri))!.instance.states[0].style).toBe("Regular");
			await underline.click();
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].underline).toBe(false);
		});

		test("the font family accepts free text and suggests the bundled fonts", async ({ page, tauri }) => {
			const family = page.getByTestId("instance-editor-family");
			await expect(family).toHaveValue("Arial");
			await family.fill("Comic Neue");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].family).toBe("Comic Neue");
			const suggestions = await page.locator("#families option").evaluateAll((options) => options.map((option) => (option as HTMLOptionElement).value));
			expect(suggestions).toEqual(["Pixeloid Sans", "Liberation Sans", "Archivo Black", "Comic Neue", "Courier Prime", "Tinos", "Anton", "Liberation Serif", "Open Sans", "Fira Sans"]);
			await expect(page.locator("#families option").first()).toHaveText("Pixeloid Sans (Pixel)");
		});

		test("the text colour picker sets states[n].colour", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-colour").fill("#ff8800");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].colour).toBe("#ff8800");
		});

		test("alignment offers top, middle and bottom", async ({ page, tauri }) => {
			const alignment = page.getByTestId("instance-editor-alignment");
			await expect(alignment.locator("option")).toHaveText(["Top", "Middle", "Bottom"]);
			await expect(alignment).toHaveValue("middle");
			await alignment.selectOption("top");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].alignment).toBe("top");
			await alignment.selectOption("bottom");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].alignment).toBe("bottom");
		});

		test("size is a number input starting at the state's size", async ({ page, tauri }) => {
			const size = page.getByTestId("instance-editor-size");
			await expect(size).toHaveValue("16");
			await expect(size).toHaveAttribute("min", "1");
			await size.fill("30");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].size).toBe(30);
		});
	});

	test.describe("image", () => {
		test("Choose image opens a file chooser", async ({ page }) => {
			const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("instance-editor-choose-image").click()]);
			expect(chooser.isMultiple()).toBe(false);
		});

		test("clicking the preview also opens the file chooser", async ({ page }) => {
			const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.getByTestId("instance-editor-preview-button").click()]);
			expect(chooser).toBeTruthy();
		});

		test("a chosen image is letterboxed to 288x288, stored as a PNG data URL and previewed", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-file").setInputFiles({ name: "wide.png", mimeType: "image/png", buffer: await PNG_RED_2X1(page) });
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toMatch(/^data:image\/png;base64,/);
			const stored = (await lastSetState(tauri))!.instance.states[0].image as string;
			await expect(page.getByTestId("instance-editor-preview")).toHaveAttribute("src", stored);
			// 2x1 source: centred band over the full width, transparent above and below.
			const sample = await page.evaluate(async (url) => {
				const t = (window as any).__t;
				const canvas = await t.toCanvas(url);
				return { size: [canvas.width, canvas.height], top: t.px(canvas, 144, 10), middle: t.px(canvas, 144, 144) };
			}, stored);
			expect(sample.size).toEqual([288, 288]);
			expect(sample.top[3]).toBe(0);
			expect(sample.middle).toEqual([255, 0, 0, 255]);
		});

		test("the chosen image replaces only the selected state's image", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-state").selectOption("1");
			await page.getByTestId("instance-editor-file").setInputFiles({ name: "x.png", mimeType: "image/png", buffer: await PNG_RED_2X1(page) });
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[1].image).toMatch(/^data:image\/png/);
			expect((await lastSetState(tauri))!.instance.states[0].image).toBe("state-one.png");
		});

		test("an animated GIF is kept as is and flagged", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-file").setInputFiles({ name: "anim.gif", mimeType: "image/gif", buffer: Buffer.from(GIF.split(",")[1]!, "base64") });
			await expect(page.getByTestId("instance-editor-gif-badge")).toHaveText("GIF");
			await expect(page.getByTestId("instance-editor-state-badge")).toHaveCount(0);
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toMatch(/^data:image\/gif;base64,/);
		});

		test("Solid colour opens the colour picker", async ({ page }) => {
			await page.getByTestId("instance-editor-colour-input").evaluate((node) => {
				(window as any).__pickerOpened = false;
				(node as HTMLInputElement).click = () => ((window as any).__pickerOpened = true);
			});
			await page.getByTestId("instance-editor-solid-colour").click();
			expect(await page.evaluate(() => (window as any).__pickerOpened)).toBe(true);
		});

		test("picking a solid colour stores a 1x1 PNG of that colour", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-colour-input").fill("#336699");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toMatch(/^data:image\/png;base64,/);
			const stored = (await lastSetState(tauri))!.instance.states[0].image as string;
			const pixel = await page.evaluate(async (url) => {
				const t = (window as any).__t;
				const canvas = await t.toCanvas(url);
				return { size: [canvas.width, canvas.height], px: t.px(canvas, 0, 0) };
			}, stored);
			expect(pixel).toEqual({ size: [1, 1], px: [0x33, 0x66, 0x99, 255] });
		});

		test("Reset restores the action's own image for that state", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-colour-input").fill("#336699");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toMatch(/^data:image\/png/);
			await page.getByTestId("instance-editor-reset").click();
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toBe("state-one.png");
			await expect(page.getByTestId("instance-editor-preview")).toHaveAttribute("src", /state-one\.png$/);
		});

		test("right-clicking the preview resets the image without opening a menu", async ({ page, tauri }) => {
			await page.getByTestId("instance-editor-colour-input").fill("#336699");
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toMatch(/^data:image\/png/);
			await page.getByTestId("instance-editor-preview-button").click({ button: "right" });
			await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toBe("state-one.png");
			await expect(page.getByTestId("key-context-menu")).toHaveCount(0);
		});

		test("the key behind the editor re-renders with the new image", async ({ page }) => {
			const canvasUrl = () =>
				key(page)
					.getByTestId("key-canvas")
					.evaluate((node) => (node as HTMLCanvasElement).toDataURL());
			await expect.poll(canvasUrl).toBeTruthy();
			const before = await canvasUrl();
			await page.getByTestId("instance-editor-colour-input").fill("#0000ff");
			await expect.poll(canvasUrl).not.toBe(before);
			const centre = await key(page)
				.getByTestId("key-canvas")
				.evaluate((node) => Array.from((node as HTMLCanvasElement).getContext("2d")!.getImageData(63, 63, 1, 1).data));
			expect(centre).toEqual([0, 0, 255, 255]);
		});
	});
});

test.describe("InstanceEditor: actions without custom state data", () => {
	test.use({ options: { seed: [{ controller: "Keypad", position: 2, action: KEY_ACTION }] } });

	test("opens from a double-state-free action with a single state entry", async ({ page }) => {
		await openApp(page);
		await openEditor(page);
		await expect(page.getByTestId("instance-editor-state").locator("option")).toHaveText(["State 1"]);
		await expect(page.getByTestId("instance-editor-show-text")).not.toBeChecked();
	});

	test("Reset falls back to an empty image when the action state has none", async ({ page, tauri }) => {
		await openApp(page);
		await openEditor(page);
		await page.getByTestId("instance-editor-colour-input").fill("#00ff00");
		await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toMatch(/^data:image\/png/);
		await page.getByTestId("instance-editor-reset").click();
		await expect.poll(async () => (await lastSetState(tauri))?.instance.states[0].image).toBe("");
	});
});
