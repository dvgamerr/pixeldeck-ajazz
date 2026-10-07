<script lang="ts">
	import type { DeviceInfo } from "$lib/DeviceInfo";
	import type { Profile } from "$lib/Profile";

	import Gear from "phosphor-svelte/lib/Gear";
	import Star from "phosphor-svelte/lib/Star";
	import DeviceStartupImage from "./DeviceStartupImage.svelte";
	import Popup from "./Popup.svelte";
	import PopupHeader from "./PopupHeader.svelte";
	import Tooltip from "./Tooltip.svelte";

	import { getPausedProfileRenderingDevices, resumeProfileRendering } from "$lib/profileRendering";
	import { settings } from "$lib/settings";
	import { inspectedInstance, inspectedParentAction } from "$lib/propertyInspector";
	import { PRODUCT_NAME, profileManager } from "$lib/singletons";

	import { invoke } from "@tauri-apps/api/core";
	import { listen } from "@tauri-apps/api/event";
	import { onMount } from "svelte";

	export let device: DeviceInfo | undefined = undefined;

	let showPopup: boolean;
	let buildInfo: string;
	let activeTab: "general" | "startup-image" = "general";
	$: startupImageDevice = device?.startup_image ? device : undefined;

	async function closeSettings() {
		showPopup = false;
		inspectedInstance.set(null);
		inspectedParentAction.set(null);

		const deviceIds = new Set(getPausedProfileRenderingDevices());
		if (device?.id) deviceIds.add(device.id);
		for (const deviceId of deviceIds) {
			try {
				const selectedProfile = await invoke<Profile>("reload_selected_profile", { device: deviceId });
				if (device?.id == deviceId) $profileManager?.applySelectedProfile(selectedProfile);
			} catch {
				// The selected device can disconnect while Settings is closing.
			} finally {
				resumeProfileRendering(deviceId);
			}
		}
	}

	function updateTheme(darktheme: boolean) {
		const theme = darktheme ? "dark" : "light";
		document.documentElement.dataset.theme = theme;
		document.documentElement.classList.toggle("dark", darktheme);
		document.querySelectorAll<HTMLIFrameElement>('iframe[title="Property inspector"]').forEach((iframe) => {
			iframe.contentWindow?.postMessage({ event: "theme", theme }, "*");
		});
	}

	onMount(() => {
		let disposed = false;
		void invoke<string>("get_build_info")
			.then((value) => {
				if (!disposed) buildInfo = value;
			})
			.catch(() => {});
		const unsubscribeSettings = settings.subscribe((value) => {
			if (value) updateTheme(value.darktheme);
		});
		let unlistenBrightness: (() => void) | undefined;
		void listen("device_brightness", ({ payload }: { payload: { action: string; value: number } }) => {
			if (!$settings) return;
			let value = $settings.brightness;
			switch (payload.action) {
				case "increase":
					value += payload.value;
					break;
				case "decrease":
					value -= payload.value;
					break;
				default:
					value = payload.value;
					break;
			}
			$settings.brightness = Math.max(0, Math.min(100, value));
		})
			.then((unlisten) => {
				if (disposed) unlisten();
				else unlistenBrightness = unlisten;
			})
			.catch(() => {});

		return () => {
			disposed = true;
			unsubscribeSettings();
			unlistenBrightness?.();
		};
	});
</script>

<button type="button" class="btn btn-ghost btn-sm" data-testid="settings-open" title="Open settings" on:click={() => (showPopup = true)}>
	<Gear size="16" weight="bold" />
	<span>Settings</span>
</button>

<svelte:window
	on:keydown={(event) => {
		if (event.key == "Escape" && showPopup) void closeSettings();
	}}
/>

<Popup show={showPopup} fullscreen onClose={closeSettings} testid="settings-popup">
	<div class="flex h-full min-h-0 min-w-0 flex-col overflow-x-hidden" data-testid="settings-view">
		<PopupHeader eyebrow="OpenDeck" title="Settings" closeLabel="Close settings" onClose={closeSettings} testid="settings-header" />

		<div class="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
			<div role="tablist" class="tabs tabs-border mt-3">
				<button
					type="button"
					role="tab"
					class:tab-active={activeTab == "general"}
					class="tab"
					data-testid="settings-tab-general"
					aria-selected={activeTab == "general"}
					on:click={() => (activeTab = "general")}>General</button
				>
				<button
					type="button"
					role="tab"
					class:tab-active={activeTab == "startup-image"}
					class="tab"
					data-testid="settings-tab-startup-image"
					aria-selected={activeTab == "startup-image"}
					on:click={() => (activeTab = "startup-image")}
				>
					Startup image
				</button>
			</div>

			{#if activeTab == "general" && $settings}
				<div class="mt-4 grid min-h-0 min-w-0 flex-1 gap-4 overflow-x-hidden overflow-y-auto xl:grid-cols-2">
					<section class="settings-card">
						<div class="card-body">
							<h3 class="card-title">Appearance &amp; device</h3>
							<label class="form-control grid grid-cols-[minmax(10rem,1fr)_auto] items-center gap-3">
								<span class="label-text">Language</span>
								<select data-testid="settings-language" bind:value={$settings.language} class="select select-sm w-40">
									<option value="en">English</option>
									<option value="es">Español</option>
									<option value="zh_CN">中文</option>
									<option value="fr">Français</option>
									<option value="de">Deutsch</option>
									<option value="ja">日本語</option>
									<option value="ko">韓国語</option>
								</select>
							</label>
							<p class="ui-caption ui-muted -mt-2">
								{PRODUCT_NAME} itself is not translated; this controls supported plugin text.
							</p>
							<label class="form-control grid grid-cols-[minmax(10rem,1fr)_minmax(10rem,1fr)] items-center gap-3">
								<span class="label-text">Device brightness</span>
								<input type="range" data-testid="settings-brightness" min="0" max="100" bind:value={$settings.brightness} class="range range-primary range-sm" />
							</label>
							<label class="settings-row">
								<span class="label-text">Dark theme</span>
								<input type="checkbox" data-testid="settings-darktheme" bind:checked={$settings.darktheme} class="toggle toggle-primary" />
							</label>
						</div>
					</section>

					<section class="settings-card">
						<div class="card-body">
							<h3 class="card-title">Startup &amp; privacy</h3>
							<label class="settings-row">
								<span class="label-text">Run in background</span>
								<input type="checkbox" data-testid="settings-background" bind:checked={$settings.background} class="toggle toggle-primary" />
							</label>
							<label class="settings-row">
								<span class="label-text">Start at login</span>
								<input type="checkbox" data-testid="settings-autolaunch" bind:checked={$settings.autolaunch} class="toggle toggle-primary" />
							</label>
							<label class="settings-row">
								<span class="label-text">Check for updates</span>
								<input type="checkbox" data-testid="settings-updatecheck" bind:checked={$settings.updatecheck} class="toggle toggle-primary" />
							</label>
							<label class="settings-row">
								<span class="label-text">Contribute statistics</span>
								<input type="checkbox" data-testid="settings-statistics" bind:checked={$settings.statistics} class="toggle toggle-primary" />
							</label>
						</div>
					</section>

					<section class="settings-card xl:col-span-2">
						<div class="card-body">
							<h3 class="card-title">Advanced</h3>
							{#if !buildInfo?.includes("windows")}
								<label class="settings-row settings-row--tip">
									<span class="label-text">Create separate Wine prefixes</span>
									<Tooltip>Each plugin receives a separate Wine prefix, which can use around 300 MB when initialized.</Tooltip>
									<input type="checkbox" data-testid="settings-separatewine" bind:checked={$settings.separatewine} class="toggle toggle-primary" />
								</label>
							{/if}
							<label class="settings-row settings-row--tip">
								<span class="label-text">Developer mode</span>
								<Tooltip>Enables plugin development tools and exposes local file paths through the local webserver.</Tooltip>
								<input type="checkbox" data-testid="settings-developer" bind:checked={$settings.developer} class="toggle toggle-primary" />
							</label>
							<label class="settings-row settings-row--tip">
								<span class="label-text">Disable device discovery</span>
								<Tooltip>Allows connected devices to be managed by other software.</Tooltip>
								<input type="checkbox" data-testid="settings-disabledevices" bind:checked={$settings.disabledevices} class="toggle toggle-primary" />
							</label>
						</div>
					</section>
				</div>
			{/if}

			{#if activeTab == "startup-image"}
				<div class="relative mt-4 flex min-h-0 min-w-0 flex-1">
					{#if startupImageDevice}
						<DeviceStartupImage device={startupImageDevice} />
					{:else}
						<section class="card border border-base-300 bg-base-200">
							<div class="card-body items-center py-16 text-center">
								<h3 class="card-title">No supported device selected</h3>
								<p class="ui-muted max-w-lg">Connect and select an Ajazz device to configure its startup image.</p>
							</div>
						</section>
					{/if}
				</div>
			{/if}

			{#if activeTab == "general"}
				<footer class="mt-4 flex flex-wrap items-center gap-2 border-t border-base-300 pt-3">
					<button type="button" class="btn btn-sm" data-testid="settings-open-config" on:click={() => invoke("open_config_directory")}>Open config directory</button>
					<button type="button" class="btn btn-sm" data-testid="settings-open-logs" on:click={() => invoke("open_log_directory")}>Open log directory</button>
					<span class="ui-caption ui-muted ml-2">{@html buildInfo}</span>
					<div class="ui-muted ml-auto flex items-center gap-1">
						<span>Please leave a</span>
						<button type="button" on:click={() => invoke("open_url", { url: "https://github.com/dvgamerr/pixeldeck-ajazz" })} class="link link-primary">star on GitHub</button>
						<Star weight="fill" class="text-warning" />
					</div>
				</footer>
			{/if}
		</div>
	</div>
</Popup>
