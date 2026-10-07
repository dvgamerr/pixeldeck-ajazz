<script lang="ts">
	import type { ActionInstance } from "$lib/ActionInstance";
	import type { Context } from "$lib/Context";
	import type { DeviceInfo } from "$lib/DeviceInfo";
	import type { Profile } from "$lib/Profile";

	import Key from "./Key.svelte";

	import { inspectedInstance, inspectedParentAction, openContextMenu } from "$lib/propertyInspector";
	import { beginInitialDeviceRender, cancelDeviceFrames } from "$lib/deviceFrames";

	import { invoke } from "@tauri-apps/api/core";
	import { onDestroy, onMount } from "svelte";

	export let device: DeviceInfo;
	export let profile: Profile;

	export let selectedDevice: string;
	const AKP05_KEY_RENDER_SIZE = 126;
	const actionMime = "application/x-opendeck-action";
	const previewShadowGutter = 32;
	$: keySize = device.id.startsWith("sd-") && device.rows == 4 && device.columns == 8 ? 192 : 144;
	let previewViewport: HTMLDivElement;
	let chassis: HTMLDivElement;
	let previewScale = 1;
	let renderProfile = "";

	$: {
		const nextRenderProfile = `${device.id}:${profile.id}`;
		if (nextRenderProfile != renderProfile) {
			renderProfile = nextRenderProfile;
			beginInitialDeviceRender(device.id, profile.id, device.rows * device.columns + device.encoders);
		}
	}

	onDestroy(() => cancelDeviceFrames(device.id));

	function updatePreviewScale() {
		if (!previewViewport || !chassis) return;

		// Keep enough space for the chassis shadow without adding layout padding.
		const availableWidth = Math.max(0, previewViewport.clientWidth - previewShadowGutter * 2);
		const availableHeight = Math.max(0, previewViewport.clientHeight - previewShadowGutter * 2);
		const naturalWidth = chassis.offsetWidth;
		const naturalHeight = chassis.offsetHeight;
		if (!naturalWidth || !naturalHeight) return;
		if (!availableWidth || !availableHeight) {
			previewScale = 0;
			return;
		}

		previewScale = Math.min(1, availableWidth / naturalWidth, availableHeight / naturalHeight);
	}

	onMount(() => {
		const resizeObserver = new ResizeObserver(updatePreviewScale);
		resizeObserver.observe(previewViewport);
		resizeObserver.observe(chassis);
		updatePreviewScale();

		return () => resizeObserver.disconnect();
	});

	function handleDragStart({ dataTransfer }: DragEvent, controller: string, position: number) {
		if (!dataTransfer) return;
		openContextMenu.set(null);
		dataTransfer.effectAllowed = "copyMove";
		dataTransfer?.setData("controller", controller);
		dataTransfer?.setData("position", position.toString());
	}

	function handleDragOver(event: DragEvent) {
		if (!event.dataTransfer) return false;
		event.preventDefault();
		event.dataTransfer.dropEffect = event.dataTransfer.types.includes("controller") ? "move" : "copy";
		return true;
	}

	async function handleDrop(event: DragEvent, controller: string, position: number) {
		event.preventDefault();
		event.stopPropagation();
		openContextMenu.set(null);
		const { dataTransfer } = event;
		if (!dataTransfer) return;

		let context = { device: device.id, profile: profile.id, controller, position };
		let array = controller == "Encoder" ? profile.sliders : profile.keys;
		const serializedAction = dataTransfer.getData(actionMime) || dataTransfer.getData("action");
		if (serializedAction) {
			let action = JSON.parse(serializedAction);
			if (!action.controllers?.includes(controller)) return;
			if (array[position]) {
				await invoke("remove_instance", { context: array[position]!.context });
			}
			const instance: ActionInstance | null = await invoke("create_instance", { context, action });
			if (instance) {
				array[position] = instance;
				profile = profile;
			}
		} else if (dataTransfer.getData("controller")) {
			let oldArray = dataTransfer.getData("controller") == "Encoder" ? profile.sliders : profile.keys;
			let oldPosition = parseInt(dataTransfer.getData("position"));
			if (oldArray == array && oldPosition == position) return;
			let response: ActionInstance = await invoke("move_instance", {
				source: { device: device.id, profile: profile.id, controller: dataTransfer.getData("controller"), position: oldPosition },
				destination: context,
				retain: false,
			});
			if (response) {
				array[position] = response;
				oldArray[oldPosition] = null;
				profile = profile;
			}
		}
	}

	async function handlePaste(source: Context, destination: Context) {
		try {
			const response: ActionInstance = await invoke("move_instance", { source, destination, retain: true });
			if (response) {
				(destination.controller == "Encoder" ? profile.sliders : profile.keys)[destination.position] = response;
				profile = profile;
			}
		} catch (error) {
			// The backend rejects pasting across controller types; leave the slot untouched.
			console.warn("Paste rejected:", error);
		}
	}
</script>

{#key device}
	<!-- svelte-ignore a11y-no-static-element-interactions -->
	<div
		bind:this={previewViewport}
		data-testid="device-view"
		data-device-id={device.id}
		class="relative h-full min-h-0 w-full min-w-0 overflow-visible"
		class:hidden={$inspectedParentAction || selectedDevice != device.id}
		on:click={() => inspectedInstance.set(null)}
		on:keyup={() => inspectedInstance.set(null)}
	>
		<div bind:this={chassis} data-testid="device-chassis" class="device-chassis flex flex-col rounded-[2rem] border border-base-300 p-6" style={`--preview-scale: ${previewScale};`}>
			<div class="relative z-[1] flex flex-col" data-testid="device-key-grid">
				{#each { length: device.rows } as _, r}
					<div class="flex flex-row">
						{#each { length: device.columns } as _, c}
							<Key
								context={{ device: device.id, profile: profile.id, controller: "Keypad", position: r * device.columns + c }}
								bind:inslot={profile.keys[r * device.columns + c]}
								on:dragover={handleDragOver}
								on:drop={(event) => handleDrop(event, "Keypad", r * device.columns + c)}
								on:dragstart={(event) => handleDragStart(event, "Keypad", r * device.columns + c)}
								{handlePaste}
								size={keySize}
								renderWidth={device.type == 7 ? AKP05_KEY_RENDER_SIZE : undefined}
								renderHeight={device.type == 7 ? AKP05_KEY_RENDER_SIZE : undefined}
							/>
						{/each}
					</div>
				{/each}
			</div>

			{#if device.type == 7}
				<div data-testid="device-touch-strip" class="touch-strip relative z-[1] mt-4 flex overflow-hidden rounded-lg border-[3px] border-neutral-700 bg-black shadow-inner">
					{#each { length: device.encoders } as _, i}
						<Key
							context={{ device: device.id, profile: profile.id, controller: "Encoder", position: i }}
							bind:inslot={profile.sliders[i]}
							on:dragover={handleDragOver}
							on:drop={(event) => handleDrop(event, "Encoder", i)}
							on:dragstart={(event) => handleDragStart(event, "Encoder", i)}
							{handlePaste}
							appearance="touch"
							renderWidth={176}
							renderHeight={112}
						/>
					{/each}
				</div>
				<div class="relative z-[1] mt-4 flex" aria-hidden="true" data-testid="device-knobs">
					{#each { length: device.encoders } as _}
						<div class="flex w-40 justify-center">
							<div class="device-knob size-[4.4rem] rounded-full border border-[#101010]"></div>
						</div>
					{/each}
				</div>
			{:else}
				<div class="flex flex-row" data-testid="device-encoder-row">
					{#each { length: device.encoders } as _, i}
						<Key
							context={{ device: device.id, profile: profile.id, controller: "Encoder", position: i }}
							bind:inslot={profile.sliders[i]}
							on:dragover={handleDragOver}
							on:drop={(event) => handleDrop(event, "Encoder", i)}
							on:dragstart={(event) => handleDragStart(event, "Encoder", i)}
							{handlePaste}
							size={keySize}
						/>
					{/each}
				</div>
			{/if}
		</div>
	</div>
{/key}
