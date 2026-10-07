<script lang="ts">
	import ArrowClockwise from "phosphor-svelte/lib/ArrowClockwise";
	import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
	import CloudArrowDown from "phosphor-svelte/lib/CloudArrowDown";
	import FileArrowUp from "phosphor-svelte/lib/FileArrowUp";
	import Gear from "phosphor-svelte/lib/Gear";
	import MagnifyingGlass from "phosphor-svelte/lib/MagnifyingGlass";
	import PuzzlePiece from "phosphor-svelte/lib/PuzzlePiece";
	import Trash from "phosphor-svelte/lib/Trash";
	import WarningCircle from "phosphor-svelte/lib/WarningCircle";
	import ListedPlugin from "./ListedPlugin.svelte";
	import PluginDetails from "./PluginDetails.svelte";
	import Popup from "./Popup.svelte";
	import PopupHeader from "./PopupHeader.svelte";
	import ReleaseAssetChooser from "./ReleaseAssetChooser.svelte";
	import Tooltip from "./Tooltip.svelte";

	import { getFetch, isInstallableAsset, matchesQuery, releasesEndpoint, sortInstalledPlugins, type GitHubPlugin, type ReleaseAsset } from "$lib/plugins";
	import { getWebserverUrl } from "$lib/ports";
	import { localisations, settings } from "$lib/settings";
	import { actionList, deviceSelector } from "$lib/singletons";

	import { invoke } from "@tauri-apps/api/core";
	import { onOpenUrl } from "@tauri-apps/plugin-deep-link";
	import { ask, message, open } from "@tauri-apps/plugin-dialog";
	import { onMount } from "svelte";

	const fetch = getFetch();

	let showPopup: boolean;
	onMount(() => {
		let disposed = false;
		let unlistenOpenUrl: (() => void) | undefined;
		const refreshInterval = window.setInterval(async () => {
			if (showPopup) installed = await invoke("list_plugins");
		}, 1e3);
		void onOpenUrl((urls: string[]) => {
			if (!urls[0].includes("installPlugin/")) return;
			const id = urls[0].split("installPlugin/")[1];
			if (plugins?.[id]) void installPluginGitHub(id, plugins[id]);
		}).then((unlisten) => {
			if (disposed) unlisten();
			else unlistenOpenUrl = unlisten;
		});

		return () => {
			disposed = true;
			window.clearInterval(refreshInterval);
			unlistenOpenUrl?.();
		};
	});

	async function installPlugin(name: string, url: string | null, file: string | null, fallback_id: string | null) {
		if (!file && !(await ask(`It may take a while to install the plugin.`, { title: `Install "${name}"?` }))) return;
		try {
			await invoke("install_plugin", { url, file, fallback_id });
			message(`Successfully installed "${name}".`, { title: `Installed "${name}"` });
			$actionList?.reload();
			installed = await invoke("list_plugins");
		} catch (error: any) {
			message(error, { title: `Failed to install "${name}"` });
		}
	}

	let choices: any[] | undefined;
	let choice: number;
	let finishChoice = (_: unknown) => {};
	let cancelChoice = () => {};
	async function chooseAsset(assets: any[]): Promise<any> {
		choices = assets;
		try {
			await new Promise((resolve, reject) => {
				finishChoice = resolve;
				cancelChoice = reject;
			});
		} finally {
			choices = undefined;
			finishChoice = (_: unknown) => {};
			cancelChoice = () => {};
		}
		return assets[choice];
	}

	let openDetailsView: string | null = null;
	async function installPluginGitHub(id: string, plugin: GitHubPlugin) {
		if (plugin.download_url) {
			await installPlugin(plugin.name, plugin.download_url, null, id);
			return;
		}

		let res;
		try {
			res = await (await fetch(releasesEndpoint(plugin.repository))).json();
		} catch (error: any) {
			message(error, { title: `Failed to install "${plugin.name}"` });
			return;
		}

		const assets: ReleaseAsset[] = res[0].assets.filter(isInstallableAsset);
		let selected;
		if (assets.length == 1) selected = assets[0];
		else {
			try {
				selected = await chooseAsset(assets);
			} catch {
				return;
			}
		}

		await installPlugin(plugin.name, selected.browser_download_url, null, id);
	}

	async function installPluginElgato(plugin: any) {
		await installPlugin(plugin.name, `https://plugins.amankhanna.me/rezipped/${plugin.id}.zip`, null, plugin.id);
	}

	async function installPluginFile() {
		const path = await open({ multiple: false, directory: false });
		if (!path) return;
		await installPlugin(path.replaceAll("\\", "/").split("/").at(-1) ?? path, null, path, null);
	}

	async function removePlugin(plugin: any) {
		if (!(await ask(`Are you sure you want to remove "${plugin.name}"?`, { title: `Remove "${plugin.name}"?` }))) return;
		try {
			await invoke("remove_plugin", { id: plugin.id });
			message(`Successfully removed "${plugin.name}".`, { title: `Removed "${plugin.name}"` });
			$actionList?.reload();
			$deviceSelector?.reloadProfiles();
			installed = await invoke("list_plugins");
		} catch (error: any) {
			message(error, { title: `Failed to remove "${plugin.name}"` });
		}
	}

	let installed: any[] = [];
	(async () => (installed = await invoke("list_plugins")))();

	let plugins: { [id: string]: GitHubPlugin };
	(async () => (plugins = await (await fetch("https://openactionapi.github.io/plugins/catalogue.json")).json()))();

	let query: string = "";
</script>

<button type="button" class="btn btn-ghost btn-sm" data-testid="plugins-open" title="Manage plugins" on:click={() => (showPopup = true)}>
	<PuzzlePiece size="16" weight="bold" />
	<span>Plugins</span>
</button>

<svelte:window
	on:keydown={(event) => {
		if (event.key == "Escape") {
			if (choices) cancelChoice();
			else if (openDetailsView) openDetailsView = null;
			else showPopup = false;
		}
	}}
/>

<Popup show={showPopup} fullscreen onClose={() => (showPopup = false)} testid="plugin-manager">
	<PopupHeader eyebrow="OpenDeck" title="Manage plugins" closeLabel="Close plugin manager" onClose={() => (showPopup = false)} testid="plugin-manager-header" />

	<div class="ui-section-heading mt-4">
		<h3 class="ui-title">Installed plugins</h3>
		<span class="badge badge-neutral badge-sm">{installed.length}</span>
	</div>
	<div class="plugin-grid mt-2" data-testid="plugins-installed">
		{#each sortInstalledPlugins(installed) as plugin}
			<ListedPlugin
				icon={getWebserverUrl(plugin.icon)}
				name={$localisations && $localisations[plugin.id] && $localisations[plugin.id].Name ? $localisations[plugin.id].Name : plugin.name}
				subtitle={plugin.version}
				disconnected={!plugin.registered}
				action={() => {
					if ($settings?.developer) invoke("reload_plugin", { id: plugin.id });
					else removePlugin(plugin);
				}}
				secondaryAction={() => {
					if (!plugin.registered) invoke("open_log_directory");
					else if (plugin.has_settings_interface) invoke("show_settings_interface", { plugin: plugin.id });
				}}
			>
				<svelte:fragment slot="secondary">
					{#if !plugin.registered}
						<WarningCircle size="24" color="#E5A50A" />
					{:else if plugin.has_settings_interface}
						<Gear size="24" color="#26A269" />
					{/if}
				</svelte:fragment>

				{#if $settings?.developer}
					<ArrowClockwise size="20" />
				{:else if !plugin.builtin}
					<Trash size="20" />
				{/if}
			</ListedPlugin>
		{/each}
	</div>

	<div class="ui-section-heading mt-6 justify-between">
		<h3 class="ui-title">Plugin store</h3>
		<button type="button" class="btn btn-sm" data-testid="plugins-install-file" on:click={installPluginFile}>
			<FileArrowUp />
			Install from file
		</button>
	</div>
	<label class="input input-bordered mt-2 w-full bg-base-200">
		<MagnifyingGlass size="16" class="opacity-60" />
		<input data-testid="plugins-search" bind:value={query} class="grow" placeholder="Search plugins" type="search" spellcheck="false" />
	</label>

	<div role="alert" class="alert mt-4">
		<ArrowSquareOut size="20" />
		<span>Need plugins from the Elgato Marketplace?</span>
		<button type="button" on:click={() => invoke("open_url", { url: "https://github.com/nekename/OpenDeck/wiki/0.-Elgato-Marketplace" })} class="btn btn-sm"> View instructions </button>
	</div>

	{#if !plugins}
		<div class="mt-4 space-y-2">
			<div class="skeleton h-20 w-full"></div>
			<div class="skeleton h-20 w-full"></div>
		</div>
	{:else}
		<div class="ui-section-heading mt-4">
			<h3 class="ui-label">Open-source plugins</h3>
			<Tooltip>Open-source plugins downloaded from the author's releases.</Tooltip>
		</div>
		<div class="plugin-grid" data-testid="plugins-store">
			{#each Object.entries(plugins) as [id, plugin]}
				<ListedPlugin
					icon="https://openactionapi.github.io/plugins/icons/{id}.png"
					name={plugin.name}
					subtitle={plugin.author}
					hidden={!matchesQuery(plugin.name, query)}
					action={() => (openDetailsView = id)}
				>
					<ArrowSquareOut size="20" />
				</ListedPlugin>
			{/each}
		</div>
	{/if}

	{#await fetch("https://plugins.amankhanna.me/catalogue.json")}
		<div class="skeleton mt-4 h-20 w-full"></div>
	{:then archiveRes}
		<div class="ui-section-heading mt-4">
			<h3 class="ui-label">Elgato App Store archive</h3>
			<Tooltip>Plugins archived from the Elgato App Store (now replaced by the Elgato Marketplace).</Tooltip>
		</div>
		{#await archiveRes.json() then entries}
			<div class="plugin-grid" data-testid="plugins-archive">
				{#each entries as plugin}
					<ListedPlugin
						icon="https://plugins.amankhanna.me/icons/{plugin.id}.png"
						name={plugin.name}
						subtitle={plugin.author}
						hidden={!matchesQuery(plugin.name, query)}
						action={() => installPluginElgato(plugin)}
					>
						<CloudArrowDown size="20" />
					</ListedPlugin>
				{/each}
			</div>
		{/await}
	{/await}
</Popup>

{#if openDetailsView}
	<PluginDetails
		id={openDetailsView}
		details={plugins[openDetailsView]}
		install={() => {
			// @ts-expect-error
			installPluginGitHub(openDetailsView, plugins[openDetailsView]);
		}}
		close={() => (openDetailsView = null)}
	/>
{/if}

{#if choices}
	<ReleaseAssetChooser {choices} bind:choice onCancel={cancelChoice} onInstall={() => finishChoice(null)} />
{/if}
