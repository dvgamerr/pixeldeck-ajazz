<script lang="ts">
	import type { DeviceInfo } from "$lib/DeviceInfo";
	import type { Profile } from "$lib/Profile";

	import { profileManager } from "$lib/singletons";
	import { completeStartupTask, showStartupTask } from "$lib/startup";

	import { invoke } from "@tauri-apps/api/core";
	import { listen, type UnlistenFn } from "@tauri-apps/api/event";
	import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
	import { onMount } from "svelte";

	export let devices: { [id: string]: DeviceInfo } = {};
	export let value: string;
	export let selectedProfiles: { [id: string]: Profile } = {};

	let registered = new Set<string>();
	let loadGeneration = 0;

	async function loadSelectedProfile(id: string, device: DeviceInfo, generation: number) {
		try {
			const profile: Profile = await invoke("get_selected_profile", { device: device.id });
			if (generation != loadGeneration || devices[id]?.id != device.id) return;
			selectedProfiles = { ...selectedProfiles, [id]: profile };
		} catch {
			// The device may disappear while its profile is loading.
		}
	}

	function reconcileDevices(current: { [id: string]: DeviceInfo }, generation: number) {
		const ids = new Set(Object.keys(current));
		let profilesChanged = false;
		for (const id of Object.keys(selectedProfiles)) {
			if (!ids.has(id)) {
				delete selectedProfiles[id];
				profilesChanged = true;
			}
		}
		for (const id of registered) {
			if (!ids.has(id)) registered.delete(id);
		}
		if (profilesChanged) selectedProfiles = { ...selectedProfiles };

		for (const [id, device] of Object.entries(current)) {
			if (registered.has(id)) continue;
			registered.add(id);
			void loadSelectedProfile(id, device, generation);
		}
	}

	$: {
		if (!value || !devices[value]) value = Object.keys(devices).sort()[0];
		reconcileDevices(devices, loadGeneration);
	}

	export function reloadProfiles() {
		registered = new Set();
		loadGeneration += 1;
	}

	onMount(() => {
		let disposed = false;
		const unlisteners: UnlistenFn[] = [];
		const keep = (promise: Promise<UnlistenFn>) => {
			void promise.then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
		};

		showStartupTask("devices");
		void invoke<{ [id: string]: DeviceInfo }>("get_devices")
			.then((current) => {
				if (!disposed) devices = current;
			})
			.catch((error) => console.error("Unable to detect connected devices:", error))
			.finally(() => completeStartupTask("devices"));
		keep(
			listen("devices", ({ payload }: { payload: { [id: string]: DeviceInfo } }) => {
				devices = payload;
			}),
		);
		keep(
			listen("switch_profile", async ({ payload }: { payload: { device: string; profile: string } }) => {
				if (!devices[payload.device]) return;
				try {
					const profile: Profile = await invoke("get_selected_profile", { device: payload.device });
					if (!devices[payload.device]) return;
					selectedProfiles = { ...selectedProfiles, [payload.device]: profile };
					if (payload.device == value) {
						$profileManager?.applySelectedProfile(profile);
					}
				} catch (error) {
					console.error(`Unable to refresh switched profile ${payload.profile} for device ${payload.device}:`, error);
				}
			}),
		);

		return () => {
			disposed = true;
			loadGeneration += 1;
			unlisteners.forEach((unlisten) => unlisten());
		};
	});

	let lastWindowSize = "";
	$: {
		if (devices[value]) {
			const width = Math.max(devices[value].columns, devices[value].encoders) * 132 + 392;
			const height = (devices[value].rows + Math.min(devices[value].encoders, 1)) * 132 + 288;
			const sizeKey = `${width}x${height}`;
			if (sizeKey != lastWindowSize) {
				lastWindowSize = sizeKey;
				const window = getCurrentWindow();
				void window
					.setMinSize(new LogicalSize(width, height))
					.then(async () => {
						const innerSize = await window.innerSize();
						if (innerSize.width < width || innerSize.height < height) {
							await window.setSize(new LogicalSize(width, height));
						}
					})
					.catch(() => {});
			}
		}
	}
</script>

{#if Object.keys(devices).length > 0}
	<select bind:value data-testid="device-selector" class="select select-sm w-full" aria-label="Device">
		<option value="" disabled selected>Choose a device...</option>

		{#each Object.entries(devices).sort() as [id, device]}
			<option value={id}>{device.name}</option>
		{/each}
	</select>
{/if}
