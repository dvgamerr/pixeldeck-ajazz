<script lang="ts">
	import Clipboard from "phosphor-svelte/lib/Clipboard";
	import Copy from "phosphor-svelte/lib/Copy";
	import Pencil from "phosphor-svelte/lib/Pencil";
	import Trash from "phosphor-svelte/lib/Trash";

	import { portalToBody } from "$lib/portal";

	// Viewport coordinates (clientX/clientY): the menu is position: fixed.
	export let x: number;
	export let y: number;
	export let occupied: boolean;
	export let onPaste: () => void;
	export let onEdit: () => void;
	export let onCopy: () => void;
	export let onDelete: () => void;
</script>

<ul use:portalToBody data-testid="key-context-menu" class="menu fixed z-[1000] w-36 rounded-box border border-base-300 bg-base-100 p-1 shadow-lg" style={`left: ${x}px; top: ${y}px;`}>
	{#if !occupied}
		<li>
			<button type="button" data-testid="context-menu-paste" on:click={onPaste}>
				<Clipboard size="18" />
				Paste
			</button>
		</li>
	{:else}
		<li><button type="button" data-testid="context-menu-edit" on:click={onEdit}><Pencil size="18" />Edit</button></li>
		<li><button type="button" data-testid="context-menu-copy" on:click={onCopy}><Copy size="18" />Copy</button></li>
		<li><button type="button" class="text-error" data-testid="context-menu-delete" on:click={onDelete}><Trash size="18" />Delete</button></li>
	{/if}
</ul>
