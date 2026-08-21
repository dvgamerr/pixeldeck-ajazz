<script lang="ts">
	import type { ActionInstance } from "$lib/ActionInstance";
	import type { ActionState } from "$lib/ActionState";
	import { contextsEqual, type Context } from "$lib/Context";

	import Clipboard from "phosphor-svelte/lib/Clipboard";
	import Copy from "phosphor-svelte/lib/Copy";
	import Pencil from "phosphor-svelte/lib/Pencil";
	import Trash from "phosphor-svelte/lib/Trash";
	import InstanceEditor from "./InstanceEditor.svelte";
	import LoadingSquares from "./LoadingSquares.svelte";

	import { isGifImageSource } from "$lib/imageFormat";
	import { queueDeviceFrame } from "$lib/deviceFrames";
	import { pausedProfileRenderingDevices } from "$lib/profileRendering";
	import { copiedContext, inspectedInstance, inspectedParentAction, openContextMenu } from "$lib/propertyInspector";
	import { CanvasLock, getImage, renderImage } from "$lib/rendererHelper";

	import { invoke } from "@tauri-apps/api/core";
	import { listen, type UnlistenFn } from "@tauri-apps/api/event";
	import { onMount } from "svelte";

	export let context: Context | null;

	// One-way binding for slot data.
	export let inslot: ActionInstance | null;
	let slot: ActionInstance | null;
	const update = (inslot: ActionInstance | null) => {
		if (inslot && context && inslot.context.split(".")[0] != context.device) return;
		slot = inslot;
	};
	$: update(inslot);

	export let active: boolean = true;
	export let scale: number = 1;
	export let appearance: "auto" | "key" | "encoder" | "touch" = "auto";
	export let renderWidth: number | undefined = undefined;
	export let renderHeight: number | undefined = undefined;
	let pressed: boolean = false;
	$: resolvedAppearance = appearance == "auto" ? (context?.controller == "Encoder" ? "encoder" : "key") : appearance;

	let state: ActionState | undefined;
	$: {
		if (!slot) {
			state = undefined;
		} else {
			state = slot.states[slot.current_state];
		}
	}

	function select(event: MouseEvent | KeyboardEvent) {
		if (event instanceof MouseEvent && event.ctrlKey) return;
		if (!slot) return;
		if (slot.action.uuid == "opendeck.multiaction" || slot.action.uuid == "opendeck.toggleaction") {
			inspectedParentAction.set(context);
		} else {
			inspectedInstance.set(slot.context);
		}
	}

	async function contextMenu(event: MouseEvent) {
		event.preventDefault();
		if (!active || !context) return;
		const width = 128;
		const height = slot ? 120 : 44;
		$openContextMenu = {
			context,
			x: Math.max(8, Math.min(event.clientX, window.innerWidth - width - 8)),
			y: Math.max(8, Math.min(event.clientY, window.innerHeight - height - 8)),
		};
	}

	function portalToBody(node: HTMLElement) {
		document.body.appendChild(node);

		return {
			destroy() {
				node.remove();
			},
		};
	}

	let showEditor = false;
	function edit() {
		showEditor = true;
	}

	export let handlePaste: ((source: Context, destination: Context) => void) | undefined = undefined;
	async function paste() {
		if (!$copiedContext || !context) return;
		if (handlePaste) handlePaste($copiedContext, context);
	}

	async function clear() {
		if (!slot) return;
		await invoke("remove_instance", { context: slot.context });
		if ($inspectedInstance == slot.context) inspectedInstance.set(null);
		showEditor = false;
		slot = null;
		inslot = slot;
	}

	let showAlert: boolean = false;
	let showOk: boolean = false;
	let timeouts: number[] = [];
	onMount(() => {
		let disposed = false;
		const unlisteners: UnlistenFn[] = [];
		const keep = (promise: Promise<UnlistenFn>) => {
			void promise.then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
		};

		keep(
			listen("update_state", ({ payload }: { payload: { context: string; contents: ActionInstance | null } }) => {
				if (payload.context == slot?.context) {
					// Keep the bound profile slot in sync with plugin-driven state changes.
					// Otherwise, the next profile reassignment restores stale state.
					inslot = payload.contents;
					slot = payload.contents;
				}
			}),
		);
		keep(
			listen("key_moved", ({ payload }: { payload: { context: Context; pressed: boolean } }) => {
				if (contextsEqual(context, payload.context)) pressed = payload.pressed;
			}),
		);
		keep(
			listen("show_alert", ({ payload }: { payload: string }) => {
				if (!slot || payload != slot.context) return;
				timeouts.forEach(clearTimeout);
				timeouts = [];
				showOk = false;
				showAlert = true;
				timeouts.push(window.setTimeout(() => (showAlert = false), 1.5e3));
			}),
		);
		keep(
			listen("show_ok", ({ payload }: { payload: string }) => {
				if (!slot || payload != slot.context) return;
				timeouts.forEach(clearTimeout);
				timeouts = [];
				showAlert = false;
				showOk = true;
				timeouts.push(window.setTimeout(() => (showOk = false), 1.5e3));
			}),
		);

		return () => {
			renderGeneration++;
			stopAnimation();
			disposed = true;
			unlisteners.forEach((unlisten) => unlisten());
			timeouts.forEach(clearTimeout);
			timeouts = [];
		};
	});

	let canvas: HTMLCanvasElement;
	let lock = new CanvasLock();
	const GIF_PREVIEW_INTERVAL_MS = 1000 / 30;
	const GIF_DEVICE_INTERVAL_MS = 1000 / 10;
	let animationFrame: number | undefined;
	let renderGeneration = 0;
	let cachedGifSource: string | undefined;
	let cachedGifImage: HTMLImageElement | undefined;
	let showLoadingAnimation = false;
	let loadedInstanceKey: string | undefined;
	$: profileRenderingPaused = context ? $pausedProfileRenderingDevices.has(context.device) : false;

	function stopAnimation() {
		if (animationFrame !== undefined) {
			window.cancelAnimationFrame(animationFrame);
			animationFrame = undefined;
		}
	}

	function sendDeviceFrame(frameContext: Context | null, isActive: boolean, image: string | null) {
		if (profileRenderingPaused || !isActive || !frameContext || !canvas) return;
		queueDeviceFrame(frameContext, image);
	}

	async function renderSlot(
		currentSlot: ActionInstance | null,
		currentState: ActionState | undefined,
		currentContext: Context | null,
		isActive: boolean,
		currentShowOk: boolean,
		currentShowAlert: boolean,
		currentPressed: boolean,
		width: number,
		height: number,
	) {
		const generation = ++renderGeneration;
		stopAnimation();
		if (canvas.width != width) canvas.width = width;
		if (canvas.height != height) canvas.height = height;

		const sl = structuredClone(currentSlot);
		const renderState = currentState ? structuredClone(currentState) : undefined;
		if (!sl || !renderState) {
			cachedGifSource = undefined;
			cachedGifImage = undefined;
			loadedInstanceKey = undefined;
			showLoadingAnimation = false;
			const canvasContext = canvas.getContext("2d");
			if (canvasContext) canvasContext.clearRect(0, 0, canvas.width, canvas.height);
			sendDeviceFrame(currentContext, isActive, null);
			return;
		}

		const fallback = sl.action.states[sl.current_state]?.image ?? sl.action.icon;
		const source = getImage(renderState.image, fallback);
		const isGif = isGifImageSource(source);
		let sourceImage = isGif && cachedGifSource == source ? cachedGifImage : undefined;
		if (!isGif) {
			cachedGifSource = undefined;
			cachedGifImage = undefined;
		}

		// Show the loading animation immediately for an action instance that has never
		// rendered a frame before, not on every re-render of a widget that keeps refreshing
		// its own image. The animation also stands in permanently for the alert icon whenever
		// an action has no icon to show at all, or its icon fails to decode.
		const instanceKey = `${sl.context}:${sl.action.uuid}`;
		const isNewInstance = instanceKey != loadedInstanceKey;
		if (isNewInstance) showLoadingAnimation = true;

		let missingIcon = false;
		const drawFrame = async (sendToDevice: boolean) => {
			if (generation != renderGeneration) return undefined;
			const unlock = await lock.lock();
			try {
				if (generation != renderGeneration) return undefined;
				const rendered = await renderImage(canvas, renderState, fallback, currentShowOk, currentShowAlert, true, currentPressed, sourceImage);
				sourceImage = rendered.image;
				missingIcon = rendered.iconUnavailable;
			} finally {
				unlock();
			}

			if (generation != renderGeneration) return undefined;
			if (sendToDevice) sendDeviceFrame(currentContext, isActive, canvas.toDataURL("image/jpeg"));
			return sourceImage;
		};

		const image = await drawFrame(true);
		if (generation == renderGeneration) {
			if (isNewInstance) loadedInstanceKey = instanceKey;
			showLoadingAnimation = missingIcon;
		}
		if (generation != renderGeneration || !isGif || !image) return;

		cachedGifSource = source;
		cachedGifImage = image;
		let lastPreviewFrame = performance.now();
		let lastDeviceFrame = lastPreviewFrame;
		const animate = async (timestamp: number) => {
			if (generation != renderGeneration) return;
			if (timestamp - lastPreviewFrame >= GIF_PREVIEW_INTERVAL_MS) {
				const sendToDevice = timestamp - lastDeviceFrame >= GIF_DEVICE_INTERVAL_MS;
				await drawFrame(sendToDevice);
				if (generation != renderGeneration) return;
				lastPreviewFrame = timestamp;
				if (sendToDevice) lastDeviceFrame = timestamp;
			}
			animationFrame = window.requestAnimationFrame(animate);
		};
		animationFrame = window.requestAnimationFrame(animate);
	}

	export let size = 144;
	$: canvasWidth = renderWidth ?? size;
	$: canvasHeight = renderHeight ?? size;
	$: canvasStyle = resolvedAppearance == "touch" ? "width: 160px; height: 100px;" : `box-sizing: content-box; width: ${size}px; height: ${size}px; transform: scale(${(112 / size) * scale});`;
	$: if (canvas) {
		if (profileRenderingPaused) {
			renderGeneration++;
			stopAnimation();
		} else {
			void renderSlot(slot, state, context, active, showOk, showAlert, pressed, canvasWidth, canvasHeight);
		}
	}
</script>

<div class="relative inline-block">
	<canvas
		bind:this={canvas}
		class={`key-canvas key-canvas--${resolvedAppearance} relative block outline-none outline-offset-2 outline-primary`}
		class:-m-2={resolvedAppearance != "touch"}
		class:border-2={resolvedAppearance != "touch"}
		class:rounded-md={resolvedAppearance == "key"}
		class:outline-solid={slot && $inspectedInstance == slot.context}
		class:-m-[2.06rem]={resolvedAppearance != "touch" && size == 192}
		class:rounded-full!={resolvedAppearance == "encoder"}
		width={canvasWidth}
		height={canvasHeight}
		style={canvasStyle}
		draggable={slot != null}
		on:dragstart
		on:dragover
		on:drop
		on:click|stopPropagation={select}
		on:keyup|stopPropagation={select}
		on:contextmenu={contextMenu}
	></canvas>

	{#if showLoadingAnimation}
		<div class="pointer-events-none absolute inset-0 flex items-center justify-center">
			<LoadingSquares size={resolvedAppearance == "touch" ? 20 : 24} class="text-white/60" />
		</div>
	{/if}
</div>

{#if $openContextMenu && contextsEqual($openContextMenu.context, context)}
	<ul use:portalToBody class="menu fixed z-[1000] w-36 rounded-box border border-base-300 bg-base-100 p-1 shadow-lg" style={`left: ${$openContextMenu.x}px; top: ${$openContextMenu.y}px;`}>
		{#if !slot}
			<li>
				<button type="button" on:click={paste}>
					<Clipboard size="18" />
					Paste
				</button>
			</li>
		{:else}
			<li><button type="button" on:click={edit}><Pencil size="18" />Edit</button></li>
			<li><button type="button" on:click={() => copiedContext.set(context)}><Copy size="18" />Copy</button></li>
			<li><button type="button" class="text-error" on:click={clear}><Trash size="18" />Delete</button></li>
		{/if}
	</ul>
{/if}

{#if slot && showEditor}
	<InstanceEditor bind:instance={slot} bind:showEditor />
{/if}
