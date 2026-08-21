<script lang="ts">
	export let size = 26;
	export let duration = 1.6;
	let className = "";
	export { className as class };

	// Corners in counter-clockwise order, starting top-left.
	const squares = [
		{ area: "tl", offset: 0 },
		{ area: "bl", offset: 0.25 },
		{ area: "br", offset: 0.5 },
		{ area: "tr", offset: 0.75 },
	];
</script>

<div class={`loading-squares ${className}`} style={`width: ${size}px; height: ${size}px;`} role="status" aria-label="Loading">
	{#each squares as square (square.area)}
		<span class={`loading-squares__cell loading-squares__cell--${square.area}`} style={`animation-duration: ${duration}s; animation-delay: ${duration * square.offset}s;`}></span>
	{/each}
</div>

<style>
	.loading-squares {
		display: grid;
		grid-template-columns: 1fr 1fr;
		grid-template-rows: 1fr 1fr;
		gap: 14%;
	}

	.loading-squares__cell {
		background: currentColor;
		opacity: 0.5;
		border-radius: 22%;
		transform: scale(1);
		animation-name: loading-squares-pulse;
		animation-timing-function: ease-in-out;
		animation-iteration-count: infinite;
	}

	.loading-squares__cell--tl {
		grid-area: 1 / 1;
	}
	.loading-squares__cell--tr {
		grid-area: 1 / 2;
	}
	.loading-squares__cell--bl {
		grid-area: 2 / 1;
	}
	.loading-squares__cell--br {
		grid-area: 2 / 2;
	}

	@keyframes loading-squares-pulse {
		0%,
		100% {
			opacity: 0.5;
			transform: scale(1);
		}
		50% {
			opacity: 1;
			transform: scale(1.35);
		}
	}
</style>
