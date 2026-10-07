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

export type RenderImageOptions = {
	canvas: HTMLCanvasElement;
	state: ActionState;
	fallback?: string;
	showOk?: boolean;
	showAlert?: boolean;
	processImage?: boolean;
	pressed?: boolean;
	sourceImage?: HTMLImageElement;
};

type RenderImageResult = { image: HTMLImageElement | undefined; iconUnavailable: boolean };

function clearCanvas(context: CanvasRenderingContext2D, canvas: HTMLCanvasElement) {
	context.clearRect(0, 0, canvas.width, canvas.height);
}

async function drawBaseImage(context: CanvasRenderingContext2D, canvas: HTMLCanvasElement, { state, fallback, processImage, sourceImage }: RenderImageOptions): Promise<RenderImageResult> {
	const resolvedSource = processImage ? getImage(state.image, fallback) : state.image;
	// No icon configured, or the configured icon failed to decode. Report this back so the
	// caller can show a loading animation in place of the alert icon instead of drawing it here.
	if (resolvedSource == "/alert.png") {
		clearCanvas(context, canvas);
		return { image: undefined, iconUnavailable: true };
	}
	try {
		const image = sourceImage ?? (await loadImage(resolvedSource));
		clearCanvas(context, canvas);
		context.imageSmoothingQuality = "high";
		context.drawImage(image, 0, 0, canvas.width, canvas.height);
		return { image, iconUnavailable: false };
	} catch (error: any) {
		if (!(error instanceof Event)) console.error(error);
		clearCanvas(context, canvas);
		return { image: undefined, iconUnavailable: true };
	}
}

function buildFont(state: ActionState, size: number): string {
	return (state.style.includes("Bold") ? "bold " : "") + (state.style.includes("Italic") ? "italic " : "") + `${size}px "${state.family}", sans-serif`;
}

function drawUnderline(context: CanvasRenderingContext2D, colour: string, x: number, y: number, width: number) {
	// Set to black for the outline, since it uses the same fill style info as the text colour.
	context.fillStyle = "black";
	context.fillRect(x - width / 2 - 3, y, width + 6, 9);
	// Reset to the user's choice of text colour.
	context.fillStyle = colour;
	context.fillRect(x - width / 2, y + 4, width, 3);
}

async function drawText(context: CanvasRenderingContext2D, canvas: HTMLCanvasElement, state: ActionState, scale: number) {
	const size = state.size * 2 * scale;
	context.textAlign = "center";
	context.font = buildFont(state, size);
	// Canvas may draw with the fallback if a bundled font has not loaded yet.
	// Loading is cached by the browser after the first render.
	await document.fonts.load(context.font).catch(() => []);
	context.fillStyle = state.colour;
	context.strokeStyle = "black";
	context.lineWidth = 3 * scale;
	context.textBaseline = "top";
	const x = canvas.width / 2;
	const lines = state.text.split("\n");
	let y = canvas.height / 2 - size * lines.length * 0.5;
	if (state.alignment === "top") y = -(size * 0.2);
	else if (state.alignment === "bottom") y = canvas.height - size * lines.length - context.lineWidth;
	lines.forEach((line, index) => {
		const lineY = y + size * index;
		context.strokeText(line, x, lineY);
		context.fillText(line, x, lineY);
		if (state.underline) drawUnderline(context, state.colour, x, lineY + size, context.measureText(line).width);
	});
}

async function drawOverlay(context: CanvasRenderingContext2D, canvas: HTMLCanvasElement, source: string) {
	const overlay = document.createElement("img");
	overlay.crossOrigin = "anonymous";
	overlay.src = source;
	await new Promise((resolve) => {
		overlay.onload = resolve;
	});
	context.drawImage(overlay, 0, 0, canvas.width, canvas.height);
}

// Make the image smaller while the button is pressed.
function shrinkForPress(context: CanvasRenderingContext2D, canvas: HTMLCanvasElement) {
	const smallCanvas = document.createElement("canvas");
	smallCanvas.width = canvas.width;
	smallCanvas.height = canvas.height;
	const smallContext = smallCanvas.getContext("2d");
	if (!smallContext) return;
	const margin = 0.1;
	smallContext.drawImage(canvas, canvas.width * margin, canvas.height * margin, canvas.width * (1 - margin * 2), canvas.height * (1 - margin * 2));
	clearCanvas(context, canvas);
	context.drawImage(smallCanvas, 0, 0);
}

export async function renderImage(options: RenderImageOptions): Promise<RenderImageResult> {
	let { canvas } = options;
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

	const result = await drawBaseImage(context, canvas, options);
	if (options.state.show) await drawText(context, canvas, options.state, scale);
	if (options.showOk) await drawOverlay(context, canvas, "/ok.png");
	if (options.showAlert) await drawOverlay(context, canvas, "/alert.png");
	if (options.pressed) shrinkForPress(context, canvas);
	return result;
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
