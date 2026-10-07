<script lang="ts">
	export let choices: { name: string }[];
	export let choice: number;
	export let onCancel: () => void;
	export let onInstall: () => void;
</script>

<div class="modal modal-open z-[300]" data-testid="asset-chooser">
	<div class="modal-box w-96 border border-base-300">
		<h3 class="ui-title">Choose a release asset</h3>
		<select class="select select-bordered mt-3 w-full" data-testid="asset-chooser-select" bind:value={choice}>
			{#each choices as option, i}
				<option value={i}>{option.name}</option>
			{/each}
		</select>
		<div class="modal-action">
			<button type="button" class="btn" data-testid="asset-chooser-cancel" on:click={onCancel}>Cancel</button>
			<button type="button" class="btn btn-primary" data-testid="asset-chooser-install" on:click={onInstall}>Install</button>
		</div>
	</div>
</div>
