<script lang="ts">
	export let icon: string;
	export let name: string;
	export let subtitle: string;
	export let hidden: boolean = false;
	export let disconnected: boolean = false;
	export let action: () => void;
	export let secondaryAction: (() => void) | undefined = undefined;
</script>

<article data-testid="plugin-item" data-plugin-name={name} class="card card-side m-1 min-w-0 border border-base-300 bg-base-100" class:hidden>
	<figure class="shrink-0 p-3 pr-0">
		<img src={icon} class="size-16 rounded-box object-cover" class:opacity-50={disconnected} alt={name} loading="lazy" />
	</figure>
	<div class="card-body min-w-0 gap-1 p-3" class:opacity-50={disconnected}>
		<h3 class="ui-label truncate">{name}</h3>
		<p class="ui-caption ui-muted truncate">{subtitle}</p>
	</div>

	<div class="flex shrink-0 flex-col justify-center gap-1 pr-2">
		{#if secondaryAction}
			<button type="button" class="btn btn-circle btn-ghost btn-sm" aria-label="Plugin settings" data-testid="plugin-secondary-action" on:click={secondaryAction}>
				<slot name="secondary" />
			</button>
		{/if}
		<button type="button" class="btn btn-circle btn-ghost btn-sm" aria-label="Plugin action" data-testid="plugin-action" on:click={action}>
			<slot />
		</button>
	</div>
</article>
