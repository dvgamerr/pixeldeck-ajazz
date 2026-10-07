<script lang="ts">
	import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
	import DownloadSimple from "phosphor-svelte/lib/DownloadSimple";
	import Popup from "./Popup.svelte";

	import "$lib/shims.ts";
	import { getFetch } from "$lib/plugins";

	import { invoke } from "@tauri-apps/api/core";
	import DOMPurify from "dompurify";
	import { marked } from "marked";
	import { baseUrl } from "marked-base-url";
	import { onMount } from "svelte";

	export let id: string;
	export let details: { repository: string; name: string; author: string; download_url: string | undefined };
	let readme = "<strong>Loading plugin details...</strong>";
	let downloadCount = 0;

	export let install: () => void;
	export let close: () => void;

	const fetch = getFetch();

	async function getReadme(repo: string): Promise<string> {
		const renderer = new marked.Renderer();
		renderer.link = function (token) {
			const rendered = marked.Renderer.prototype.link.call(this, token);
			return rendered.replace("<a", '<a target="_blank" rel="noreferrer"');
		};
		marked.use({ renderer });
		const urls = [
			"https://raw.githubusercontent.com/" + repo + "/main/README.md",
			"https://raw.githubusercontent.com/" + repo + "/main/readme.md",
			"https://raw.githubusercontent.com/" + repo + "/master/README.md",
			"https://raw.githubusercontent.com/" + repo + "/master/readme.md",
		];
		for (const url of urls) {
			const response = await fetch(url);
			if (response.ok) {
				marked.use(baseUrl(url));
				return DOMPurify.sanitize(await marked.parse(await response.text()), { ADD_ATTR: ["target"] });
			}
		}
		return DOMPurify.sanitize(await marked.parse("**Plugin README file not found**\n\n[View plugin on GitHub](https://github.com/" + repo + ")"), { ADD_ATTR: ["target"] });
	}

	onMount(async () => {
		const repo = details.repository.split("/")[3] + "/" + details.repository.split("/")[4];

		readme = await getReadme(repo);

		const releasesResponse = await fetch("https://api.github.com/repos/" + repo + "/releases");
		const releases = await releasesResponse.json();
		for (const release of releases) {
			for (const asset of release.assets) {
				downloadCount += asset.download_count;
			}
		}
	});
</script>

<Popup show fullscreen onClose={close} testid="plugin-details">
	<header class="flex justify-end">
		<button type="button" class="btn btn-circle btn-ghost" aria-label="Close plugin details" data-testid="plugin-details-close" on:click={close}>✕</button>
	</header>
	<section class="card card-side border border-base-300 bg-base-200">
		<figure class="shrink-0 p-4">
			<img src={"https://openactionapi.github.io/plugins/icons/" + id + ".png"} alt={details.name} class="size-32 rounded-box object-cover" />
		</figure>
		<div class="card-body justify-center">
			<h2 class="ui-page-title" data-testid="plugin-details-name">{details.name}</h2>
			<div class="ui-muted flex items-center gap-2">
				<span>by</span>
				<img src={"https://avatars.githubusercontent.com/" + details.repository.split("/")[3]} alt="" class="avatar size-7 rounded-full" />
				<a
					target="_blank"
					rel="noreferrer"
					href={"https://github.com/" + details.repository.split("/")[3]}
					on:click={() => window.open("https://github.com/" + details.repository.split("/")[3])}
					class="link link-primary"
					data-testid="plugin-details-author"
				>
					{details.author}
					{#if details.repository.split("/")[3] != details.author}
						({details.repository.split("/")[3]})
					{/if}
				</a>
			</div>

			<div class="card-actions mt-3 items-center">
				<div class="join">
					<button type="button" data-testid="plugin-details-install" on:click={install} class="btn btn-primary join-item">Install</button>
					<button
						type="button"
						on:click={() => invoke("open_url", { url: details.download_url ?? details.repository + "/releases/latest" })}
						class="btn btn-primary join-item"
						aria-label="Open latest release"
						data-testid="plugin-details-open-release"
					>
						<ArrowSquareOut size={20} />
					</button>
				</div>

				{#if downloadCount}
					<span class="badge badge-ghost gap-1" data-testid="plugin-details-downloads"><DownloadSimple size={16} />{downloadCount}</span>
				{/if}
			</div>
		</div>
	</section>

	<article data-testid="plugin-details-readme" class="plugin-readme mt-4 rounded-box border border-base-300 bg-base-100 p-4">
		{@html readme}
	</article>
</Popup>
