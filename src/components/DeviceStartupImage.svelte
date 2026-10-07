<script lang="ts">
	import type { DeviceInfo } from "$lib/DeviceInfo";

	import ArrowClockwise from "phosphor-svelte/lib/ArrowClockwise";
	import Plus from "phosphor-svelte/lib/Plus";
	import Stack from "phosphor-svelte/lib/Stack";
	import StartupImageLayerList from "./StartupImageLayerList.svelte";
	import StartupImageMask from "./StartupImageMask.svelte";

	import { pauseProfileRendering, resumeProfileRendering } from "$lib/profileRendering";
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
		type PersistedLayer,
		type StartupImageProject,
	} from "$lib/startupImage";

	import { invoke } from "@tauri-apps/api/core";
	import { onMount } from "svelte";

	export let device: DeviceInfo;

	let fileInput: HTMLInputElement;
	let previewCanvas: HTMLCanvasElement;
	let editorViewport: HTMLDivElement;
	let previewPanel: HTMLElement;
	let previewDisplayWidth = 0;
	let layers: ImageLayer[] = [];
	let activeLayerId = "";
	let activeLayer: ImageLayer | undefined;
	let revision = 0;
	let savedRevision = 0;
	let loading = true;
	let applying = false;
	let successMessage = "";
	let errorMessage = "";
	let disposed = false;
	let lastDeviceId = "";
	let loadGeneration = 0;
	let dragPointerId: number | undefined;
	let dragStartX = 0;
	let dragStartY = 0;
	let dragStartOffsetX = 0;
	let dragStartOffsetY = 0;
	let resizePointerId: number | undefined;
	let resizeAnchorX = 0;
	let resizeAnchorY = 0;
	let resizeStartVectorX = 0;
	let resizeStartVectorY = 0;
	let resizeStartVectorLengthSquared = 1;
	let resizeStartWidth = 0;
	let resizeStartHeight = 0;
	let resizeDirectionX = 0;
	let resizeDirectionY = 0;
	let resizeStartZoom = 1;
	let rotatePointerId: number | undefined;
	let rotateCenterX = 0;
	let rotateCenterY = 0;
	let rotateStartAngle = 0;
	let rotateStartValue = 0;
	$: startupImage = device.startup_image ?? { width: 0, height: 0 };
	$: showAkp05Mask = device.type == 7 && startupImage.width == AKP05_MASK.width && startupImage.height == AKP05_MASK.height;
	$: previewContentWidth = showAkp05Mask ? AKP05_MASK.width : startupImage.width;
	$: previewContentHeight = showAkp05Mask ? AKP05_MASK.height : startupImage.height;
	$: previewFrameWidth = previewContentWidth + PREVIEW_PADDING * 2;
	$: previewFrameHeight = previewContentHeight + PREVIEW_PADDING * 2;
	$: activeLayer = layers.find((layer) => layer.id == activeLayerId);
	$: transformBounds = getTransformBounds(activeLayer, startupImage.width, startupImage.height);
	$: isDirty = revision != savedRevision;
	$: if (previewPanel && previewFrameWidth && previewFrameHeight) updatePreviewDisplaySize();
	$: if (previewCanvas && startupImage.width && startupImage.height) {
		drawComposedImage(previewCanvas, layers, startupImage);
	}
	$: if (device.id != lastDeviceId) {
		lastDeviceId = device.id;
		void loadProject(device.id, ++loadGeneration);
	}

	function updatePreviewDisplaySize() {
		if (!previewPanel || !previewFrameWidth || !previewFrameHeight) return;
		const availableWidth = Math.max(0, previewPanel.clientWidth - PREVIEW_PANEL_HORIZONTAL_PADDING);
		const availableHeight = Math.max(0, previewPanel.clientHeight - PREVIEW_PANEL_VERTICAL_PADDING);
		previewDisplayWidth = Math.max(0, Math.min(previewFrameWidth, availableWidth, availableHeight * (previewFrameWidth / previewFrameHeight)));
	}

	onMount(() => {
		disposed = false;
		const resizeObserver = new ResizeObserver(updatePreviewDisplaySize);
		resizeObserver.observe(previewPanel);
		updatePreviewDisplaySize();
		return () => {
			disposed = true;
			resizeObserver.disconnect();
		};
	});

	function clearEditor() {
		layers = [];
		activeLayerId = "";
		revision = 0;
		savedRevision = 0;
		successMessage = "";
		errorMessage = "";
		applying = false;
		if (fileInput) fileInput.value = "";
	}

	async function loadProject(deviceId: string, generation: number) {
		clearEditor();
		loading = true;
		try {
			const project = await invoke<StartupImageProject>("get_startup_image_project", { device: deviceId });
			const loadedLayers = await Promise.all(
				project.layers.map(async (layer) => ({
					...layer,
					decoded: await decodeImage(layer.image),
				})),
			);
			if (generation != loadGeneration || device.id != deviceId) return;
			layers = loadedLayers;
			activeLayerId = loadedLayers.at(-1)?.id ?? "";
			revision = 0;
			savedRevision = 0;
		} catch (error) {
			if (generation != loadGeneration || device.id != deviceId) return;
			errorMessage = `Unable to load saved startup image: ${String(error)}`;
		} finally {
			if (generation == loadGeneration && device.id == deviceId) loading = false;
		}
	}

	async function selectFiles(fileList: FileList | null) {
		successMessage = "";
		errorMessage = "";
		const files = Array.from(fileList ?? []);
		if (!files.length) return;
		if (layers.length + files.length > MAX_LAYERS) {
			errorMessage = `A startup image can contain at most ${MAX_LAYERS} images.`;
			return;
		}

		const { validFiles, rejected } = partitionFiles(files);

		try {
			const addedLayers = await Promise.all(
				validFiles.map(async ({ file, extension }) => {
					const image = await readFile(file, extension);
					return {
						id: makeLayerId(),
						name: file.name,
						image,
						zoom: 1,
						offset_x: 0,
						offset_y: 0,
						rotation: 0,
						decoded: await decodeImage(image),
					} satisfies ImageLayer;
				}),
			);
			layers = [...layers, ...addedLayers];
			activeLayerId = addedLayers.at(-1)?.id ?? activeLayerId;
			if (addedLayers.length) revision += 1;
		} catch (error) {
			rejected.push(String(error));
		}

		if (rejected.length) errorMessage = rejected.join(". ");
		if (fileInput) fileInput.value = "";
	}

	function openFilePicker() {
		fileInput.value = "";
		fileInput.click();
	}

	function updateActiveLayer(changes: Partial<PersistedLayer>) {
		if (!activeLayer) return;
		layers = layers.map((layer) => (layer.id == activeLayerId ? { ...layer, ...changes } : layer));
		revision += 1;
		successMessage = "";
		errorMessage = "";
	}

	function resetPlacement() {
		updateActiveLayer({ zoom: 1, offset_x: 0, offset_y: 0, rotation: 0 });
	}

	function removeLayer(id: string) {
		const index = layers.findIndex((layer) => layer.id == id);
		if (index < 0) return;
		layers = layers.filter((layer) => layer.id != id);
		if (activeLayerId == id) activeLayerId = layers[Math.min(index, layers.length - 1)]?.id ?? "";
		revision += 1;
		successMessage = "";
		errorMessage = "";
	}

	function moveLayer(id: string, direction: 1 | -1) {
		const index = layers.findIndex((layer) => layer.id == id);
		const destination = index + direction;
		if (index < 0 || destination < 0 || destination >= layers.length) return;
		const nextLayers = [...layers];
		[nextLayers[index], nextLayers[destination]] = [nextLayers[destination], nextLayers[index]];
		layers = nextLayers;
		revision += 1;
		successMessage = "";
		errorMessage = "";
	}

	function pointerToOutput(event: PointerEvent) {
		const bounds = editorViewport.getBoundingClientRect();
		return {
			x: ((event.clientX - bounds.left) * startupImage.width) / bounds.width,
			y: ((event.clientY - bounds.top) * startupImage.height) / bounds.height,
		};
	}

	function beginDrag(event: PointerEvent) {
		if (!activeLayer?.decoded || !editorViewport || resizePointerId != undefined || rotatePointerId != undefined) return;
		dragPointerId = event.pointerId;
		dragStartX = event.clientX;
		dragStartY = event.clientY;
		dragStartOffsetX = activeLayer.offset_x;
		dragStartOffsetY = activeLayer.offset_y;
		editorViewport.setPointerCapture(event.pointerId);
	}

	function moveImage(event: PointerEvent) {
		if (dragPointerId != event.pointerId || !editorViewport) return;
		const bounds = editorViewport.getBoundingClientRect();
		if (!bounds.width || !bounds.height) return;
		updateActiveLayer({
			offset_x: dragStartOffsetX + (event.clientX - dragStartX) * (startupImage.width / bounds.width),
			offset_y: dragStartOffsetY + (event.clientY - dragStartY) * (startupImage.height / bounds.height),
		});
	}

	function endDrag(event: PointerEvent) {
		if (dragPointerId != event.pointerId || !editorViewport) return;
		if (editorViewport.hasPointerCapture(event.pointerId)) editorViewport.releasePointerCapture(event.pointerId);
		dragPointerId = undefined;
	}

	function beginResize(event: PointerEvent, directionX: number, directionY: number) {
		if (!activeLayer?.decoded || !editorViewport || rotatePointerId != undefined) return;
		event.preventDefault();
		const fittedImage = getFittedImageSize(activeLayer.decoded, startupImage, activeLayer.zoom);
		const centerX = startupImage.width / 2 + activeLayer.offset_x;
		const centerY = startupImage.height / 2 + activeLayer.offset_y;
		const oppositeCorner = rotateVector((-directionX * fittedImage.width) / 2, (-directionY * fittedImage.height) / 2, activeLayer.rotation);
		const startVector = rotateVector(directionX * fittedImage.width, directionY * fittedImage.height, activeLayer.rotation);

		resizePointerId = event.pointerId;
		resizeAnchorX = centerX + oppositeCorner.x;
		resizeAnchorY = centerY + oppositeCorner.y;
		resizeStartVectorX = startVector.x;
		resizeStartVectorY = startVector.y;
		resizeStartVectorLengthSquared = Math.max(1, startVector.x ** 2 + startVector.y ** 2);
		resizeStartWidth = fittedImage.width;
		resizeStartHeight = fittedImage.height;
		resizeDirectionX = directionX;
		resizeDirectionY = directionY;
		resizeStartZoom = activeLayer.zoom;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
	}

	function resizeImage(event: PointerEvent) {
		if (resizePointerId != event.pointerId || !editorViewport || !activeLayer) return;
		const pointer = pointerToOutput(event);
		const pointerVectorX = pointer.x - resizeAnchorX;
		const pointerVectorY = pointer.y - resizeAnchorY;
		const projectedScale = (pointerVectorX * resizeStartVectorX + pointerVectorY * resizeStartVectorY) / resizeStartVectorLengthSquared;
		const nextZoom = Math.max(0.25, Math.min(3, resizeStartZoom * projectedScale));
		const appliedScale = nextZoom / resizeStartZoom;
		const centerFromAnchor = rotateVector((resizeDirectionX * resizeStartWidth * appliedScale) / 2, (resizeDirectionY * resizeStartHeight * appliedScale) / 2, activeLayer.rotation);

		updateActiveLayer({
			zoom: nextZoom,
			offset_x: resizeAnchorX + centerFromAnchor.x - startupImage.width / 2,
			offset_y: resizeAnchorY + centerFromAnchor.y - startupImage.height / 2,
		});
	}

	function endResize(event: PointerEvent) {
		if (resizePointerId == event.pointerId) resizePointerId = undefined;
	}

	function beginRotate(event: PointerEvent) {
		if (!activeLayer?.decoded || !editorViewport || resizePointerId != undefined) return;
		event.preventDefault();
		const bounds = editorViewport.getBoundingClientRect();
		rotatePointerId = event.pointerId;
		rotateCenterX = bounds.left + (0.5 + activeLayer.offset_x / startupImage.width) * bounds.width;
		rotateCenterY = bounds.top + (0.5 + activeLayer.offset_y / startupImage.height) * bounds.height;
		rotateStartAngle = Math.atan2(event.clientY - rotateCenterY, event.clientX - rotateCenterX);
		rotateStartValue = activeLayer.rotation;
		(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
	}

	function rotateImage(event: PointerEvent) {
		if (rotatePointerId != event.pointerId) return;
		const angle = Math.atan2(event.clientY - rotateCenterY, event.clientX - rotateCenterX);
		updateActiveLayer({ rotation: rotateStartValue + ((angle - rotateStartAngle) * 180) / Math.PI });
	}

	function endRotate(event: PointerEvent) {
		if (rotatePointerId == event.pointerId) rotatePointerId = undefined;
	}

	function moveTransform(event: PointerEvent) {
		resizeImage(event);
		rotateImage(event);
	}

	function endTransform(event: PointerEvent) {
		endResize(event);
		endRotate(event);
	}

	function persistedProject(): StartupImageProject {
		return {
			layers: layers.map(({ id, name, image, zoom, offset_x, offset_y, rotation }) => ({
				id,
				name,
				image,
				zoom,
				offset_x,
				offset_y,
				rotation,
			})),
		};
	}

	async function applyImage() {
		if (!layers.length || layers.some((layer) => !layer.decoded) || !device.startup_image || applying) return;
		applying = true;
		successMessage = "";
		errorMessage = "";
		const deviceId = device.id;
		const deviceName = device.name;
		const revisionToSave = revision;
		let projectSaved = false;
		let profileRenderingPaused = false;
		try {
			await invoke("save_startup_image_project", { device: deviceId, project: persistedProject() });
			projectSaved = true;
			if (revision == revisionToSave) savedRevision = revisionToSave;
			if (disposed || device.id != deviceId) return;

			const output = document.createElement("canvas");
			drawComposedImage(output, layers, startupImage);
			profileRenderingPaused = pauseProfileRendering(deviceId);
			await invoke("set_startup_image", { device: deviceId, image: output.toDataURL("image/jpeg", 0.92) });
			successMessage = `Saved and applied to ${deviceName}.`;
		} catch (error) {
			if (profileRenderingPaused) resumeProfileRendering(deviceId);
			errorMessage = projectSaved ? `Saved, but could not apply to the device: ${String(error)}` : `Unable to save startup image: ${String(error)}`;
		} finally {
			applying = false;
		}
	}
</script>

<svelte:window on:pointermove={moveTransform} on:pointerup={endTransform} on:pointercancel={endTransform} />

<div class="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-3" data-testid="startup-image-editor">
	<input
		bind:this={fileInput}
		type="file"
		data-testid="startup-file-input"
		class="hidden"
		accept=".png,.jpg,.jpeg,.bmp,.svg,image/png,image/jpeg,image/bmp,image/svg+xml"
		multiple
		on:change={() => selectFiles(fileInput.files)}
	/>

	<header class="flex shrink-0 flex-wrap items-start gap-2">
		<div class="min-w-0">
			<div class="flex items-center gap-2">
				<Stack size="20" weight="bold" class="text-primary" />
				<h3 class="ui-title">Startup composition</h3>
			</div>
			<p class="ui-muted mt-1">Add multiple images, select a layer, then drag, resize, or rotate it on the device layout.</p>
		</div>
		<div class="ml-auto flex flex-wrap items-center justify-end gap-2">
			{#if loading}
				<span class="badge badge-outline gap-2"><span class="loading loading-spinner loading-xs"></span>Loading</span>
			{:else if successMessage}
				<span class="badge badge-success">Applied &amp; saved</span>
			{:else if layers.length && isDirty}
				<span class="badge badge-warning badge-outline">Unsaved changes</span>
			{:else if layers.length}
				<span class="badge badge-success badge-outline">Saved</span>
			{:else}
				<span class="badge badge-outline">No images</span>
			{/if}
			<button type="button" class="btn btn-sm" data-testid="startup-add" disabled={loading || layers.length >= MAX_LAYERS} on:click={openFilePicker}>
				<Plus size="16" weight="bold" />
				Add
			</button>
			<button type="button" class="btn btn-ghost btn-sm" data-testid="startup-reset" disabled={!activeLayer || applying} on:click={resetPlacement}>Reset selected</button>
			<button type="button" class="btn btn-primary btn-sm min-w-32" data-testid="startup-apply" disabled={!layers.length || loading || applying} on:click={applyImage}>
				{#if applying}
					<span class="loading loading-spinner loading-sm"></span>
					Applying…
				{:else}
					Apply
				{/if}
			</button>
		</div>
	</header>

	{#if errorMessage}
		<div role="alert" class="alert alert-error shrink-0">
			<span>{errorMessage}</span>
		</div>
	{/if}

	<div class="grid min-h-0 min-w-0 flex-1 grid-cols-[15rem_minmax(0,1fr)] overflow-hidden rounded-box border border-base-300 bg-base-200">
		<StartupImageLayerList {layers} {activeLayerId} {loading} onSelect={(id) => (activeLayerId = id)} onMove={moveLayer} onRemove={removeLayer} onAdd={openFilePicker} />

		<section bind:this={previewPanel} class="relative flex min-h-0 min-w-0 items-center justify-center overflow-hidden p-3">
			<div class="relative shrink-0 overflow-visible rounded-lg bg-black shadow-lg" style={`width: ${previewDisplayWidth}px; aspect-ratio: ${previewFrameWidth} / ${previewFrameHeight};`}>
				<!-- svelte-ignore a11y-no-static-element-interactions -->
				<div
					bind:this={editorViewport}
					class="absolute touch-none overflow-visible bg-black"
					class:cursor-grab={activeLayer && dragPointerId == undefined && resizePointerId == undefined && rotatePointerId == undefined}
					class:cursor-grabbing={dragPointerId != undefined}
					style={`left: ${(PREVIEW_PADDING / previewFrameWidth) * 100}%; top: ${(PREVIEW_PADDING / previewFrameHeight) * 100}%; width: ${(startupImage.width / previewFrameWidth) * 100}%; height: ${(startupImage.height / previewFrameHeight) * 100}%;`}
					on:pointerdown={beginDrag}
					on:pointermove={moveImage}
					on:pointerup={endDrag}
					on:pointercancel={endDrag}
				>
					<canvas bind:this={previewCanvas} data-testid="startup-preview-canvas" class="absolute inset-0 h-full w-full rounded-lg"></canvas>

					{#if showAkp05Mask}
						<StartupImageMask />
					{/if}

					{#if activeLayer?.decoded}
						<div
							class="pointer-events-none absolute z-20 border border-primary shadow-[0_0_0_1px_rgba(255,255,255,0.45)]"
							style={`left: ${transformBounds.left * 100}%; top: ${transformBounds.top * 100}%; width: ${transformBounds.width * 100}%; height: ${transformBounds.height * 100}%; transform: rotate(${activeLayer.rotation}deg);`}
						>
							{#each RESIZE_HANDLES as handle}
								<button
									type="button"
									class={`pointer-events-auto absolute rounded-full border border-primary bg-white shadow-sm ${handle.placement}`}
									aria-label={handle.label}
									on:pointerdown|stopPropagation={(event) => beginResize(event, handle.x, handle.y)}
								></button>
							{/each}
							<span class="absolute top-full left-1/2 h-6 w-px -translate-x-1/2 bg-primary"></span>
							<button
								type="button"
								class="pointer-events-auto absolute top-[calc(100%+1.25rem)] left-1/2 z-30 flex size-7 -translate-x-1/2 cursor-grab items-center justify-center rounded-full border border-primary-content/20 bg-primary text-primary-content shadow-md active:cursor-grabbing"
								aria-label="Rotate selected image"
								on:pointerdown|stopPropagation={beginRotate}
							>
								<ArrowClockwise size="16" weight="bold" />
							</button>
						</div>
					{/if}
				</div>
			</div>

			{#if !loading && !layers.length}
				<div class="pointer-events-none absolute inset-0 flex items-end justify-center pb-4">
					<p class="ui-caption ui-muted rounded-full border border-base-300 bg-base-100/90 px-3 py-1.5 backdrop-blur">The black frame represents the device canvas.</p>
				</div>
			{/if}
		</section>
	</div>
</div>
