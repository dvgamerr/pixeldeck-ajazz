<script lang="ts">
	import CaretDown from "phosphor-svelte/lib/CaretDown";
	import CaretUp from "phosphor-svelte/lib/CaretUp";
	import ImageSquare from "phosphor-svelte/lib/ImageSquare";
	import Plus from "phosphor-svelte/lib/Plus";
	import Trash from "phosphor-svelte/lib/Trash";

	import type { ImageLayer } from "$lib/startupImage";

	export let layers: ImageLayer[];
	export let activeLayerId: string;
	export let loading: boolean;
	export let onSelect: (id: string) => void;
	export let onMove: (id: string, direction: 1 | -1) => void;
	export let onRemove: (id: string) => void;
	export let onAdd: () => void;
</script>

<aside class="flex min-h-0 flex-col border-r border-base-300 bg-base-100/60" data-testid="startup-layer-list">
	<div class="flex shrink-0 items-center gap-2 border-b border-base-300 p-3">
		<ImageSquare size="17" weight="bold" />
		<h4 class="ui-label">Images</h4>
		<span class="badge badge-sm ml-auto">{layers.length}</span>
	</div>

	{#if layers.length}
		<div class="min-h-0 flex-1 space-y-2 overflow-y-auto p-2">
			{#each [...layers].reverse() as layer (layer.id)}
				{@const layerIndex = layers.findIndex((item) => item.id == layer.id)}
				<div
					data-testid="startup-layer"
					class={`group flex items-center gap-2 rounded-field border p-2 transition-colors ${activeLayerId == layer.id ? "border-primary bg-primary/10" : "border-base-300 bg-base-100"}`}
				>
					<button type="button" class="flex min-w-0 flex-1 items-center gap-2 text-left" data-testid="startup-layer-select" on:click={() => onSelect(layer.id)}>
						<span class="flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-md border border-base-300 bg-black">
							<img src={layer.image} alt="" class="max-h-full max-w-full object-contain" />
						</span>
						<span class="min-w-0">
							<span class="ui-label block truncate">{layer.name}</span>
							<span class="ui-caption ui-muted block">Layer {layerIndex + 1}</span>
						</span>
					</button>
					<div class="flex shrink-0 flex-col">
						<button type="button" class="btn btn-ghost btn-xs h-5 min-h-5 px-1" aria-label={`Move ${layer.name} up`} disabled={layerIndex == layers.length - 1} on:click={() => onMove(layer.id, 1)}>
							<CaretUp size="13" weight="bold" />
						</button>
						<button type="button" class="btn btn-ghost btn-xs h-5 min-h-5 px-1" aria-label={`Move ${layer.name} down`} disabled={layerIndex == 0} on:click={() => onMove(layer.id, -1)}>
							<CaretDown size="13" weight="bold" />
						</button>
					</div>
					<button type="button" class="btn btn-circle btn-ghost btn-xs text-error" aria-label={`Remove ${layer.name}`} data-testid="startup-layer-remove" on:click={() => onRemove(layer.id)}>
						<Trash size="15" />
					</button>
				</div>
			{/each}
		</div>
	{:else if loading}
		<div class="flex flex-1 items-center justify-center p-4">
			<span class="loading loading-spinner loading-md text-primary"></span>
		</div>
	{:else}
		<div class="flex flex-1 flex-col items-center justify-center p-4 text-center">
			<div class="mb-3 flex size-12 items-center justify-center rounded-full bg-base-200">
				<ImageSquare size="24" class="text-base-content/45" />
			</div>
			<p class="ui-label">No images added</p>
			<p class="ui-caption ui-muted mt-1">Add one or more PNG, JPG, JPEG, BMP, or SVG files.</p>
			<button type="button" class="btn btn-primary btn-sm mt-4" on:click={onAdd}>
				<Plus size="15" weight="bold" />
				Add images
			</button>
		</div>
	{/if}
</aside>
