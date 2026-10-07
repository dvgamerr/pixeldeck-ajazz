import { akp05Device, openApp, test, expect } from "./fixtures/tauriMock";

test.describe("AKP05E_552A layout", () => {
	test.beforeEach(async ({ page, tauri }) => {
		void tauri;
		await openApp(page);
	});

	test("shows the device name and connected badge", async ({ page }) => {
		await expect(page.getByTestId("device-title")).toHaveText(akp05Device().name);
		await expect(page.getByTestId("device-connected-badge")).toBeVisible();
		await expect(page.getByTestId("device-selector")).toBeVisible();
		await expect(page.getByTestId("profile-selector")).toHaveValue("Default");
	});

	test("renders 10 keys as 2 rows x 5 columns", async ({ page }) => {
		const keys = page.locator('[data-testid^="device-key-"][data-controller="Keypad"]');
		await expect(keys).toHaveCount(10);
		await expect(page.getByTestId("device-key-grid").locator("> div")).toHaveCount(2);

		const boxes = await Promise.all(Array.from({ length: 10 }, (_, i) => page.getByTestId(`device-key-${i}`).boundingBox()));
		const rows = new Set(boxes.map((box) => Math.round(box!.y)));
		const columns = new Set(boxes.map((box) => Math.round(box!.x)));
		expect(rows.size).toBe(2);
		expect(columns.size).toBe(5);
		// Row-major ordering: keys 0..4 on the first row, 5..9 on the second.
		expect(Math.round(boxes[0]!.y)).toBe(Math.round(boxes[4]!.y));
		expect(boxes[5]!.y).toBeGreaterThan(boxes[0]!.y);
		expect(boxes[1]!.x).toBeGreaterThan(boxes[0]!.x);
	});

	test("renders 4 touch zones as one continuous strip aligned with the key grid", async ({ page }) => {
		const zones = page.locator('[data-testid^="device-touch-"][data-controller="Encoder"]');
		await expect(zones).toHaveCount(4);
		await expect(page.getByTestId("device-touch-strip")).toBeVisible();
		// The AKP05 layout has no per-encoder key row; the strip replaces it.
		await expect(page.getByTestId("device-encoder-row")).toHaveCount(0);

		const boxes = await Promise.all([0, 1, 2, 3].map((i) => page.getByTestId(`device-touch-${i}`).boundingBox()));
		const y = Math.round(boxes[0]!.y);
		for (let i = 0; i < 4; i++) {
			expect(Math.round(boxes[i]!.y)).toBe(y);
			expect(boxes[i]!.width).toBeCloseTo(boxes[0]!.width, 0);
			if (i > 0) {
				// Adjacent zones: no gap and no overlap.
				expect(boxes[i]!.x - (boxes[i - 1]!.x + boxes[i - 1]!.width)).toBeCloseTo(0, 0);
			}
		}

		const strip = (await page.getByTestId("device-touch-strip").boundingBox())!;
		const grid = (await page.getByTestId("device-key-grid").boundingBox())!;
		const zonesWidth = boxes[3]!.x + boxes[3]!.width - boxes[0]!.x;
		// The visible strip must never be wider than the five-button grid, and the zones fill it.
		expect(strip.width).toBeCloseTo(grid.width, 0);
		expect(zonesWidth).toBeLessThanOrEqual(strip.width);
		// Strip equals the grid width (660px); the 4x160px zones leave ~14px unused on the right.
		expect(strip.width - zonesWidth).toBeLessThan(24);
		// Strip is horizontally aligned with the grid (same left edge within the chassis padding / border).
		expect(Math.abs(strip.x - grid.x)).toBeLessThan(24);
		expect(strip.y).toBeGreaterThan(grid.y + grid.height - 1);
	});

	test("renders 4 encoder knobs", async ({ page }) => {
		await expect(page.getByTestId("device-knobs").locator("> div")).toHaveCount(4);
		const knobs = await page
			.getByTestId("device-knobs")
			.locator("> div")
			.evaluateAll((nodes) => nodes.map((node) => node.getBoundingClientRect().x));
		expect(new Set(knobs.map(Math.round)).size).toBe(4);
	});

	test("touch zones use the Encoder controller and keys use Keypad", async ({ page }) => {
		for (let i = 0; i < 4; i++) {
			await expect(page.getByTestId(`device-touch-${i}`)).toHaveAttribute("data-controller", "Encoder");
		}
		for (let i = 0; i < 10; i++) {
			await expect(page.getByTestId(`device-key-${i}`)).toHaveAttribute("data-controller", "Keypad");
		}
	});

	test("the touch canvas element is 160x100 and the property inspector is hidden by default", async ({ page }) => {
		const canvas = page.getByTestId("device-touch-0").getByTestId("key-canvas");
		const box = (await canvas.boundingBox())!;
		expect(Math.round(box.width)).toBe(160);
		expect(Math.round(box.height)).toBe(100);
		await expect(page.getByTestId("property-inspector-panel")).toBeHidden();
	});
});

test.describe("generic (non type 7) device", () => {
	test.use({ options: { devices: { "sd-STD": { id: "sd-STD", name: "Generic Deck", rows: 3, columns: 5, encoders: 2, type: 0 } } } });

	test("uses an encoder row instead of the touch strip", async ({ page }) => {
		await openApp(page);
		await expect(page.locator('[data-testid^="device-key-"][data-controller="Keypad"]')).toHaveCount(15);
		await expect(page.getByTestId("device-encoder-row")).toBeVisible();
		await expect(page.getByTestId("device-touch-strip")).toHaveCount(0);
		await expect(page.locator('[data-testid^="device-encoder-"][data-controller="Encoder"]')).toHaveCount(2);
	});
});

test.describe("no devices", () => {
	test.use({ options: { devices: {} } });

	test("shows the empty state", async ({ page }) => {
		await page.goto("/");
		await expect(page.getByTestId("app-main")).toBeVisible();
		await expect(page.getByTestId("device-view")).toHaveCount(0);
		await expect(page.getByTestId("device-selector")).toHaveCount(0);
		await expect(page.getByTestId("device-title")).toHaveText("OpenDeck");
	});
});
