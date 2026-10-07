<script lang="ts">
	import { PRODUCT_NAME } from "$lib/singletons";

	import { invoke } from "@tauri-apps/api/core";

	let buildInfo: string;
	(async () => (buildInfo = await invoke("get_build_info")))();
</script>

<div class="flex h-full w-full items-center justify-center text-center" data-testid="no-devices">
	<div class="card w-96 border border-base-300 bg-base-100">
		<div class="card-body items-center p-6">
			<div class="flex size-10 items-center justify-center rounded-box bg-base-200 text-xl">⌁</div>
			<h2 class="card-title">No devices detected</h2>
			<p class="ui-muted">Make sure your devices are connected properly and you have permission to access them.</p>
			{#if buildInfo?.includes("linux")}
				<div role="alert" class="alert alert-warning py-2 text-left" data-testid="no-devices-udev-hint">
					<span>Ensure you have the correct udev subsystem rules installed.</span>
				</div>
			{/if}
			<p class="ui-muted">You may need to install a plugin that adds support for your device.</p>
			<div class="card-actions mt-2">
				<button type="button" class="btn btn-primary btn-sm" data-testid="no-devices-restart" on:click={() => invoke("restart")}>Restart {PRODUCT_NAME}</button>
			</div>
		</div>
	</div>
</div>
