import type { ActionState } from "./ActionState.ts";

import { isGifImageSource } from "./imageFormat.ts";
import { getWebserverUrl } from "./ports.ts";

const MAX_DECODED_IMAGES = 256;
const decodedImages = new Map<string, Promise<HTMLImageElement>>();

function loadImage(source: string): Promise<HTMLImageElement> {
	if (!isGifImageSource(source)) {
		const cached = decodedImages.get(source);
		if (cached) return cached;
	}

	const promise = new Promise<HTMLImageElement>((resolve, reject) => {
		const image = document.createElement("img");
		image.crossOrigin = "anonymous";
		image.onload = () => resolve(image);
		image.onerror = reject;
		image.src = source;
	});

	if (!isGifImageSource(source)) {
		decodedImages.set(source, promise);
		promise.catch(() => decodedImages.delete(source));
		if (decodedImages.size > MAX_DECODED_IMAGES) {
			const oldest = decodedImages.keys().next().value;
			if (oldest !== undefined) decodedImages.delete(oldest);
		}
	}
	return promise;
}

export function getImage(image: string | undefined, fallback: string | undefined): string {
	if (!image) return fallback ? getImage(fallback, undefined) : "/alert.png";
	if (image.startsWith("opendeck/")) return image.replace("opendeck", "");
	if (!image.startsWith("data:")) return getWebserverUrl(image);
	const svgxmlre = /^data:image\/svg\+xml(?!.*?;base64.*?)(?:;[\w=]*)*,(.+)/;
	const base64re = /^data:image\/(apng|avif|gif|jpeg|png|svg\+xml|webp|bmp|x-icon|tiff);base64,([A-Za-z0-9+/]+={0,2})?/;
	if (svgxmlre.test(image)) {
		let svg = (svgxmlre.exec(image) as RegExpExecArray)[1].replace(/;$/, "");
		try {
			svg = decodeURIComponent(svg);
		} finally {
			image = "data:image/svg+xml," + encodeURIComponent(svg);
		}
	}
	if (base64re.test(image)) {
		const exec = base64re.exec(image)!;
		if (!exec[2]) return fallback ? getImage(fallback, undefined) : "/alert.png";
		else image = exec[0];
	}
	return image;
}

export class CanvasLock {
	currentLock = Promise.resolve();
	async lock() {
		let unlockNext: () => void;
		const willLock = new Promise<void>((resolve) => (unlockNext = resolve));
		const previousLock = this.currentLock;
		this.currentLock = willLock;
		await previousLock;
		return unlockNext!;
	}
}

export async function renderImage(
	canvas: HTMLCanvasElement,
	state: ActionState,
	fallback: string | undefined,
	showOk: boolean,
	showAlert: boolean,
	processImage: boolean,
	pressed: boolean,
	sourceImage?: HTMLImageElement,
): Promise<{ image: HTMLImageElement | undefined; iconUnavailable: boolean }> {
	// Create canvas
	let scale = 1;
	if (!canvas) {
		canvas = document.createElement("canvas");
		canvas.width = 144;
		canvas.height = 144;
	} else {
		scale = canvas.width / 144;
	}

	const context = canvas.getContext("2d");
	if (!context) return { image: undefined, iconUnavailable: false };
	let renderedImage: HTMLImageElement | undefined;
	// No icon configured, or the configured icon failed to decode. Report this back so the
	// caller can show a loading animation in place of the alert icon instead of drawing it here.
	let iconUnavailable = false;

	const resolvedSource = processImage ? getImage(state.image, fallback) : state.image;
	if (resolvedSource == "/alert.png") {
		iconUnavailable = true;
		context.clearRect(0, 0, canvas.width, canvas.height);
	} else {
		try {
			// Load image
			const image = sourceImage ?? (await loadImage(resolvedSource));
			renderedImage = image;

			// Draw image
			context.clearRect(0, 0, canvas.width, canvas.height);
			context.imageSmoothingQuality = "high";
			context.drawImage(image, 0, 0, canvas.width, canvas.height);
		} catch (error: any) {
			if (!(error instanceof Event)) console.error(error);
			renderedImage = undefined;
			iconUnavailable = true;
			context.clearRect(0, 0, canvas.width, canvas.height);
		}
	}

	// Draw text
	if (state.show) {
		const size = state.size * 2 * scale;
		context.textAlign = "center";
		context.font = (state.style.includes("Bold") ? "bold " : "") + (state.style.includes("Italic") ? "italic " : "") + `${size}px "${state.family}", sans-serif`;
		// Canvas may draw with the fallback if a bundled font has not loaded yet.
		// Loading is cached by the browser after the first render.
		await document.fonts.load(context.font).catch(() => []);
		context.fillStyle = state.colour;
		context.strokeStyle = "black";
		context.lineWidth = 3 * scale;
		context.textBaseline = "top";
		const x = canvas.width / 2;
		let y = canvas.height / 2 - size * state.text.split("\n").length * 0.5;
		switch (state.alignment) {
			case "top":
				y = -(size * 0.2);
				break;
			case "bottom":
				y = canvas.height - size * state.text.split("\n").length - context.lineWidth;
				break;
		}
		for (const [index, line] of Object.entries(state.text.split("\n"))) {
			context.strokeText(line, x, y + size * parseInt(index));
			context.fillText(line, x, y + size * parseInt(index));
			if (state.underline) {
				const width = context.measureText(line).width;
				// Set to black for the outline, since it uses the same fill style info as the text colour.
				context.fillStyle = "black";
				context.fillRect(x - width / 2 - 3, y + size * parseInt(index) + size, width + 6, 9);
				// Reset to the user's choice of text colour.
				context.fillStyle = state.colour;
				context.fillRect(x - width / 2, y + size * parseInt(index) + size + 4, width, 3);
			}
		}
	}

	if (showOk) {
		const okImage = document.createElement("img");
		okImage.crossOrigin = "anonymous";
		okImage.src = "/ok.png";
		await new Promise((resolve) => {
			okImage.onload = resolve;
		});
		context.drawImage(okImage, 0, 0, canvas.width, canvas.height);
	}

	if (showAlert) {
		const alertImage = document.createElement("img");
		alertImage.crossOrigin = "anonymous";
		alertImage.src = "/alert.png";
		await new Promise((resolve) => {
			alertImage.onload = resolve;
		});
		context.drawImage(alertImage, 0, 0, canvas.width, canvas.height);
	}

	// Make the image smaller while the button is pressed.
	if (pressed) {
		const smallCanvas = document.createElement("canvas");
		smallCanvas.width = canvas.width;
		smallCanvas.height = canvas.height;
		const newContext = smallCanvas.getContext("2d");
		const margin = 0.1;
		if (newContext) {
			newContext.drawImage(canvas, canvas.width * margin, canvas.height * margin, canvas.width * (1 - margin * 2), canvas.height * (1 - margin * 2));
			context.clearRect(0, 0, canvas.width, canvas.height);
			context.drawImage(smallCanvas, 0, 0);
		}
	}

	return { image: renderedImage, iconUnavailable };
}

export async function resizeImage(source: string): Promise<string | undefined> {
	// Drawing an animated GIF through a canvas keeps only its current frame. Keep
	// the original data URL so the key renderer can decode and animate every frame.
	if (isGifImageSource(source)) return source;

	const canvas = document.createElement("canvas");
	canvas.width = 288;
	canvas.height = 288;
	const context = canvas.getContext("2d");
	if (!context) return;

	const image = document.createElement("img");
	image.crossOrigin = "anonymous";
	image.src = source;
	await new Promise((resolve) => (image.onload = resolve));

	let xOffset = 0,
		yOffset = 0;
	let xScaled = canvas.width,
		yScaled = canvas.height;
	if (image.width > image.height) {
		const ratio = image.height / image.width;
		yScaled = canvas.height * ratio;
		yOffset = (canvas.height - yScaled) / 2;
	} else if (image.width < image.height) {
		const ratio = image.width / image.height;
		xScaled = canvas.width * ratio;
		xOffset = (canvas.width - xScaled) / 2;
	}

	context.imageSmoothingQuality = "high";
	context.clearRect(0, 0, canvas.width, canvas.height);
	context.drawImage(image, xOffset, yOffset, xScaled, yScaled);

	return canvas.toDataURL();
}
