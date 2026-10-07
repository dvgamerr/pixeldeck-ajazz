<script lang="ts">
	export let eyebrow: string;
	export let title: string;
	export let closeLabel: string;
	export let onClose: () => void;
	// "bordered" is the full-screen popup header; "compact" is the in-panel popup header.
	export let variant: "bordered" | "compact" = "bordered";
	export let testid: string | undefined = undefined;
</script>

<header class={variant == "bordered" ? "flex shrink-0 items-center border-b border-base-300 pb-3" : "mb-3 flex items-center"} data-testid={testid}>
	<div>
		<p class="ui-eyebrow">{eyebrow}</p>
		<h2 class="ui-page-title">{title}</h2>
	</div>
	<button
		type="button"
		class={`btn btn-circle btn-ghost ml-auto ${variant == "compact" ? "btn-sm" : ""}`}
		aria-label={closeLabel}
		data-testid={testid ? `${testid}-close` : undefined}
		on:click={onClose}>✕</button
	>
</header>
