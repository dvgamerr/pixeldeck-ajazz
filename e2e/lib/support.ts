import type { Page } from "@playwright/test";

/** Helpers installed in the page as `window.__t` (self-contained: runs in the browser). */
function installHelpers() {
	const w = window as any;
	w.__t = {
		/** A solid-colour PNG data URL. */
		solid: (colour: string, width = 8, height = 8) => {
			const canvas = document.createElement("canvas");
			canvas.width = width;
			canvas.height = height;
			const context = canvas.getContext("2d")!;
			context.fillStyle = colour;
			context.fillRect(0, 0, width, height);
			return canvas.toDataURL("image/png");
		},
		/** RGBA at a point of a canvas. */
		px: (canvas: HTMLCanvasElement, x: number, y: number) => Array.from(canvas.getContext("2d")!.getImageData(x, y, 1, 1).data),
		/** Bounding box + count of non-transparent pixels. */
		opaque: (canvas: HTMLCanvasElement) => {
			const { data, width, height } = canvas.getContext("2d")!.getImageData(0, 0, canvas.width, canvas.height);
			let count = 0,
				minX = width,
				minY = height,
				maxX = -1,
				maxY = -1;
			for (let y = 0; y < height; y++) {
				for (let x = 0; x < width; x++) {
					if (data[(y * width + x) * 4 + 3]! > 0) {
						count++;
						minX = Math.min(minX, x);
						maxX = Math.max(maxX, x);
						minY = Math.min(minY, y);
						maxY = Math.max(maxY, y);
					}
				}
			}
			return { count, minX, minY, maxX, maxY };
		},
		/** Decode a data URL into a canvas. */
		toCanvas: async (source: string) => {
			const image = new Image();
			image.src = source;
			await image.decode();
			const canvas = document.createElement("canvas");
			canvas.width = image.naturalWidth;
			canvas.height = image.naturalHeight;
			canvas.getContext("2d")!.drawImage(image, 0, 0);
			return canvas;
		},
		state: (overrides: Record<string, unknown> = {}) => ({
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
		}),
	};
}

/** Install the in-page helpers (window.__t) before any navigation. */
export function withHelpers(page: Page) {
	return page.addInitScript(installHelpers);
}

/** Open the app (so the Tauri mock is live and Vite serves /src modules) with the in-page helpers installed. */
export async function openForModules(page: Page) {
	await page.addInitScript(installHelpers);
	await page.goto("/");
	await page.getByTestId("app-main").waitFor();
}
