import DOMPurify from "dompurify";

export type PersistedLayer = {
	id: string;
	name: string;
	image: string;
	zoom: number;
	offset_x: number;
	offset_y: number;
	rotation: number;
};

export type ImageLayer = PersistedLayer & {
	decoded?: HTMLImageElement;
};

export type StartupImageProject = {
	layers: PersistedLayer[];
};

export type Size = { width: number; height: number };

const MAX_FILE_SIZE = 10 * 1024 * 1024;
export const MAX_LAYERS = 64;
export const PREVIEW_PADDING = 16;
export const PREVIEW_PANEL_HORIZONTAL_PADDING = 32;
export const PREVIEW_PANEL_VERTICAL_PADDING = 32;
const ALLOWED_EXTENSIONS = new Set(["png", "jpg", "jpeg", "bmp", "svg"]);
const ALLOWED_MIME_TYPES = new Set(["image/png", "image/jpeg", "image/bmp", "image/x-ms-bmp", "image/svg+xml"]);

// AKP05E_552A mask and startup image share one 810 × 470 coordinate space.
// The visible key apertures are 126 × 126 even though the device protocol also
// accepts larger per-surface payloads. The touch display is one continuous
// 810 × 130 strip; its four action zones are not separate physical screens.
export const AKP05_MASK = {
	width: 810,
	height: 470,
	keySize: 126,
	keyX: [0, 170, 340, 510, 680],
	keyY: [0, 170],
	touchStrip: { x: 0, y: 340, width: 810, height: 130 },
};

export const RESIZE_HANDLES = [
	{ label: "Resize from top left", placement: "-top-1.5 -left-1.5 size-3.5 cursor-nwse-resize", x: -1, y: -1 },
	{ label: "Resize from top right", placement: "-top-1.5 -right-1.5 size-3.5 cursor-nesw-resize", x: 1, y: -1 },
	{ label: "Resize from bottom left", placement: "-bottom-1.5 -left-1.5 size-3.5 cursor-nesw-resize", x: -1, y: 1 },
	{ label: "Resize from bottom right", placement: "-right-1.5 -bottom-1.5 size-3.5 cursor-nwse-resize", x: 1, y: 1 },
	{ label: "Resize from top", placement: "-top-1 left-1/2 h-2 w-5 -translate-x-1/2 cursor-ns-resize", x: 0, y: -1 },
	{ label: "Resize from bottom", placement: "-bottom-1 left-1/2 h-2 w-5 -translate-x-1/2 cursor-ns-resize", x: 0, y: 1 },
	{ label: "Resize from left", placement: "top-1/2 -left-1 h-5 w-2 -translate-y-1/2 cursor-ew-resize", x: -1, y: 0 },
	{ label: "Resize from right", placement: "top-1/2 -right-1 h-5 w-2 -translate-y-1/2 cursor-ew-resize", x: 1, y: 0 },
] as const;

export function decodeImage(source: string) {
	return new Promise<HTMLImageElement>((resolve, reject) => {
		const image = new Image();
		image.onload = () => resolve(image);
		image.onerror = () => reject(new Error("The image could not be decoded"));
		image.src = source;
	});
}

export function getFittedImageSize(image: HTMLImageElement, output: Size, scale: number): Size {
	const fillScale = Math.max(output.width / image.naturalWidth, output.height / image.naturalHeight);
	return {
		width: image.naturalWidth * fillScale * scale,
		height: image.naturalHeight * fillScale * scale,
	};
}

export function getTransformBounds(layer: ImageLayer | undefined, outputWidth: number, outputHeight: number) {
	if (!layer?.decoded || !outputWidth || !outputHeight) {
		return { left: 0, top: 0, width: 0, height: 0 };
	}
	const fittedImage = getFittedImageSize(layer.decoded, { width: outputWidth, height: outputHeight }, layer.zoom);
	return {
		left: ((outputWidth - fittedImage.width) / 2 + layer.offset_x) / outputWidth,
		top: ((outputHeight - fittedImage.height) / 2 + layer.offset_y) / outputHeight,
		width: fittedImage.width / outputWidth,
		height: fittedImage.height / outputHeight,
	};
}

export function rotateVector(x: number, y: number, degrees: number) {
	const radians = (degrees * Math.PI) / 180;
	const cosine = Math.cos(radians);
	const sine = Math.sin(radians);
	return {
		x: x * cosine - y * sine,
		y: x * sine + y * cosine,
	};
}

export function drawComposedImage(canvas: HTMLCanvasElement, imageLayers: ImageLayer[], output: Size) {
	if (!output.width || !output.height) return;
	if (canvas.width != output.width) canvas.width = output.width;
	if (canvas.height != output.height) canvas.height = output.height;

	const context = canvas.getContext("2d");
	if (!context) return;
	context.fillStyle = "#000000";
	context.fillRect(0, 0, canvas.width, canvas.height);
	context.imageSmoothingEnabled = true;
	context.imageSmoothingQuality = "high";

	for (const layer of imageLayers) {
		if (!layer.decoded) continue;
		const fittedImage = getFittedImageSize(layer.decoded, output, layer.zoom);
		context.save();
		context.translate(canvas.width / 2 + layer.offset_x, canvas.height / 2 + layer.offset_y);
		context.rotate((layer.rotation * Math.PI) / 180);
		context.drawImage(layer.decoded, -fittedImage.width / 2, -fittedImage.height / 2, fittedImage.width, fittedImage.height);
		context.restore();
	}
}

function sanitizeSvgToDataUrl(text: string, fileName: string): string {
	const sanitized = DOMPurify.sanitize(text, {
		USE_PROFILES: { svg: true, svgFilters: true },
		FORBID_TAGS: ["script", "foreignObject", "iframe", "object", "embed"],
	});
	const document = new DOMParser().parseFromString(sanitized, "image/svg+xml");
	if (document.querySelector("parsererror") || document.documentElement.localName != "svg") {
		throw new Error(`${fileName} is not a valid SVG image`);
	}
	const bytes = new TextEncoder().encode(sanitized);
	let binary = "";
	for (let index = 0; index < bytes.length; index += 8192) {
		binary += String.fromCharCode(...bytes.subarray(index, index + 8192));
	}
	return `data:image/svg+xml;base64,${btoa(binary)}`;
}

const MIME_BY_EXTENSION: Record<string, string> = { png: "image/png", bmp: "image/bmp" };

function normalizeReadResult(result: string, fileName: string, extension: string): string {
	if (extension == "svg") return sanitizeSvgToDataUrl(result, fileName);
	const mimeType = MIME_BY_EXTENSION[extension] ?? "image/jpeg";
	return result.replace(/^data:[^;,]+;base64,/, `data:${mimeType};base64,`);
}

export function readFile(file: File, extension: string) {
	return new Promise<string>((resolve, reject) => {
		const reader = new FileReader();
		const fail = () => reject(new Error(`${file.name} could not be read`));
		reader.onload = () => {
			if (typeof reader.result != "string") return fail();
			try {
				resolve(normalizeReadResult(reader.result, file.name, extension));
			} catch (error) {
				reject(error);
			}
		};
		reader.onerror = fail;
		if (extension == "svg") reader.readAsText(file);
		else reader.readAsDataURL(file);
	});
}

export function makeLayerId() {
	return globalThis.crypto?.randomUUID?.() ?? `layer-${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

export function partitionFiles(files: File[]) {
	const validFiles: { file: File; extension: string }[] = [];
	const rejected: string[] = [];
	for (const file of files) {
		const extension = file.name.split(".").pop()?.toLowerCase() ?? "";
		if (!ALLOWED_EXTENSIONS.has(extension) || (file.type && !ALLOWED_MIME_TYPES.has(file.type))) {
			rejected.push(`${file.name} has an unsupported file type`);
		} else if (file.size > MAX_FILE_SIZE) {
			rejected.push(`${file.name} is larger than 10 MB`);
		} else {
			validFiles.push({ file, extension });
		}
	}
	return { validFiles, rejected };
}
