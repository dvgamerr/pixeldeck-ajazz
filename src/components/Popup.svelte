<script lang="ts">
	export let show = false;
	export let fullscreen = false;
	export let onClose: (() => void) | undefined = undefined;
	export let testid: string | undefined = undefined;
</script>

{#if show}
	{#if fullscreen}
		<div class="modal modal-open z-[200]" data-testid={testid} role="dialog" aria-modal="true">
			<section class="modal-box h-[calc(100vh-2rem)] w-[calc(100vw-2rem)] max-w-none overflow-x-hidden overflow-y-auto border border-base-300 p-4">
				<slot />
			</section>
			<button type="button" class="modal-backdrop bg-black/60 backdrop-blur-sm" aria-label="Close modal" on:click={() => onClose?.()}>Close</button>
		</div>
	{:else}
		<section data-testid={testid} class="card absolute inset-2 z-30 overflow-auto border border-base-300 bg-base-100 shadow-lg" role="dialog" aria-modal="true">
			<div class="card-body p-4">
				<slot />
			</div>
		</section>
	{/if}
{/if}
