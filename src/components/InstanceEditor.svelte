<script lang="ts">
	import type { ActionInstance } from "$lib/ActionInstance";

	import { isGifImageSource } from "$lib/imageFormat";
	import { getImage, resizeImage } from "$lib/rendererHelper";
	import { portalToPreviewDock } from "$lib/portal";

	import { invoke } from "@tauri-apps/api/core";

	export let instance: ActionInstance;
	export let showEditor: boolean;

	let state: number = 0;
	let bold: boolean;
	let italic: boolean;

	let fileInput: HTMLInputElement;
	let colourInput: HTMLInputElement;

	function update(instance: ActionInstance, selectedState: number) {
		bold = instance.states[selectedState].style.includes("Bold");
		italic = instance.states[selectedState].style.includes("Italic");
	}

	function updateStyle() {
		instance.states[state].style = bold && italic ? "Bold Italic" : bold ? "Bold" : italic ? "Italic" : "Regular";
	}

	const FONT_FAMILIES = [
		["Pixeloid Sans", "Pixeloid Sans (Pixel)"],
		["Liberation Sans", "Liberation Sans"],
		["Archivo Black", "Archivo Black"],
		["Comic Neue", "Comic Neue"],
		["Courier Prime", "Courier Prime"],
		["Tinos", "Tinos"],
		["Anton", "Anton"],
		["Liberation Serif", "Liberation Serif"],
		["Open Sans", "Open Sans"],
		["Fira Sans", "Fira Sans"],
	];

	$: stateImage = getImage(instance.states[state].image, instance.action.states[state]?.image ?? instance.action.icon);

	function onImageFileChange() {
		if (!fileInput.files || fileInput.files.length == 0) return;
		const reader = new FileReader();

		reader.onload = async () => {
			const result = reader.result?.toString();
			if (!result) return;
			const resized = await resizeImage(result);
			instance.states[state].image = resized ?? result;
		};

		reader.readAsDataURL(fileInput.files[0]);
	}

	function onColourChange() {
		const canvas = document.createElement("canvas");
		canvas.width = 1;
		canvas.height = 1;
		const context = canvas.getContext("2d");
		if (!context) return;
		context.fillStyle = colourInput.value;
		context.fillRect(0, 0, canvas.width, canvas.height);
		instance.states[state].image = canvas.toDataURL("image/png");
	}

	function resetImage() {
		instance.states[state].image = instance.action.states[state]?.image ?? instance.action.icon;
	}
	$: update(instance, state);
	$: void invoke("set_state", { instance, state });
</script>

<svelte:window
	on:keydown={(event) => {
		if (event.key == "Escape") showEditor = false;
	}}
/>

<div use:portalToPreviewDock data-testid="instance-editor" class="modal modal-open absolute inset-0 z-[120]">
	<button type="button" class="modal-backdrop bg-black/60 backdrop-blur-sm" aria-label="Close key editor" data-testid="instance-editor-backdrop" on:click={() => (showEditor = false)}>Close</button>

	<div
		class="modal-box z-10 flex max-h-[calc(100%-2rem)] w-[min(58rem,calc(100%-2rem))] max-w-none flex-col overflow-hidden border border-base-300 bg-base-100 p-0"
		role="dialog"
		aria-modal="true"
		aria-label="Edit key appearance"
	>
		<header class="flex shrink-0 items-center gap-4 border-b border-base-300 px-5 py-4">
			<div class="min-w-0">
				<p class="ui-eyebrow">Key appearance</p>
				<h2 class="ui-page-title truncate">Customize state {state + 1}</h2>
			</div>
			<div class="ml-auto flex shrink-0 items-center gap-2">
				<label class="flex items-center gap-2">
					<span class="ui-label hidden sm:inline">State</span>
					<select class="select select-sm w-32" data-testid="instance-editor-state" bind:value={state} aria-label="State">
						{#each instance.states as _, i}
							<option value={i}>State {i + 1}</option>
						{/each}
					</select>
				</label>
				<button type="button" class="btn btn-circle btn-ghost btn-sm" aria-label="Close key editor" data-testid="instance-editor-close" on:click={() => (showEditor = false)}>✕</button>
			</div>
		</header>

		<div class="grid min-h-0 gap-5 overflow-y-auto p-5 md:grid-cols-[13rem_minmax(0,1fr)]">
			<aside class="ui-surface h-fit bg-base-200 p-4 md:sticky md:top-0">
				<div class="mb-3 flex items-center justify-between">
					<h3 class="ui-title">Preview</h3>
					{#if isGifImageSource(stateImage)}
						<span class="badge badge-primary badge-sm">GIF</span>
					{:else}
						<span class="badge badge-neutral badge-sm">State {state + 1}</span>
					{/if}
				</div>
				<button
					type="button"
					class="btn mx-auto block h-auto w-full border-0 bg-transparent p-0 shadow-none"
					aria-label="Choose state image"
					on:click={() => fileInput.click()}
					on:contextmenu={(event) => {
						event.preventDefault();
						resetImage();
					}}
				>
					<img src={stateImage} class="mx-auto aspect-square w-full max-w-40 rounded-box border border-base-300 object-cover shadow-sm" alt="State {state + 1} preview" />
				</button>
				<button type="button" on:click={() => fileInput.click()} class="btn btn-primary btn-sm mt-4 w-full" data-testid="instance-editor-choose-image">Choose image</button>
				<div class="mt-2 grid grid-cols-2 gap-2">
					<button type="button" on:click={() => colourInput.click()} class="btn btn-sm" data-testid="instance-editor-solid-colour">Solid colour</button>
					<button type="button" on:click={resetImage} class="btn btn-ghost btn-sm" data-testid="instance-editor-reset">Reset</button>
				</div>
				<p class="ui-caption ui-muted mt-3 text-center">Animated GIFs are preserved. Right-click to reset.</p>
			</aside>
			<input bind:this={fileInput} type="file" class="hidden" accept="image/*" on:change={onImageFileChange} />
			<input bind:this={colourInput} type="color" class="sr-only" value="#FFFFFE" on:change={onColourChange} />

			<div class="flex min-w-0 flex-col gap-4">
				<section class="ui-surface p-4">
					<div class="mb-3 flex items-start justify-between gap-4">
						<div>
							<h3 class="ui-title">Label</h3>
							<p class="ui-caption ui-muted mt-1">Add a short title over the state image.</p>
						</div>
						<label class="flex shrink-0 cursor-pointer items-center gap-2">
							<span class="ui-label">Show text</span>
							<input data-testid="instance-editor-show-text" type="checkbox" bind:checked={instance.states[state].show} class="toggle toggle-primary toggle-sm" />
						</label>
					</div>
					<label class="form-control">
						<span class="label-text mb-1">Text</span>
						<textarea data-testid="instance-editor-text" bind:value={instance.states[state].text} rows="3" placeholder="Enter state label" class="textarea textarea-bordered w-full resize-none"
						></textarea>
					</label>
				</section>

				<section class="ui-surface p-4">
					<div class="mb-3">
						<h3 class="ui-title">Typography</h3>
						<p class="ui-caption ui-muted mt-1">Choose the font, placement, and emphasis.</p>
					</div>
					<div class="grid gap-3 sm:grid-cols-2">
						<label class="form-control sm:col-span-2">
							<span class="label-text mb-1">Font family</span>
							<input list="families" bind:value={instance.states[state].family} placeholder="Font family" class="input input-bordered input-sm w-full" />
							<datalist id="families">
								{#each FONT_FAMILIES as [value, label]}
									<option {value}>{label}</option>
								{/each}
							</datalist>
						</label>
						<label class="form-control">
							<span class="label-text mb-1">Text colour</span>
							<input type="color" bind:value={instance.states[state].colour} class="input input-bordered h-9 w-full p-1" />
						</label>
						<label class="form-control">
							<span class="label-text mb-1">Alignment</span>
							<select bind:value={instance.states[state].alignment} class="select select-bordered select-sm w-full">
								<option value="top">Top</option>
								<option value="middle">Middle</option>
								<option value="bottom">Bottom</option>
							</select>
						</label>
						<div class="form-control">
							<span class="label-text mb-1">Style</span>
							<div class="join w-full">
								<button
									type="button"
									class="btn btn-sm join-item flex-1 font-bold"
									class:btn-active={bold}
									aria-pressed={bold}
									on:click={() => {
										bold = !bold;
										updateStyle();
									}}>B</button
								>
								<button
									type="button"
									class="btn btn-sm join-item flex-1 italic"
									class:btn-active={italic}
									aria-pressed={italic}
									on:click={() => {
										italic = !italic;
										updateStyle();
									}}>I</button
								>
								<button
									type="button"
									class="btn btn-sm join-item flex-1 underline"
									class:btn-active={instance.states[state].underline}
									aria-pressed={instance.states[state].underline}
									on:click={() => (instance.states[state].underline = !instance.states[state].underline)}>U</button
								>
							</div>
						</div>
						<label class="form-control">
							<span class="label-text mb-1">Size</span>
							<input type="number" min="1" bind:value={instance.states[state].size} class="input input-bordered input-sm w-full" />
						</label>
					</div>
				</section>
			</div>
		</div>

		<footer class="flex shrink-0 items-center gap-3 border-t border-base-300 bg-base-200/50 px-5 py-3">
			<p class="ui-caption ui-muted">Changes are applied immediately.</p>
			<button type="button" class="btn btn-primary btn-sm ml-auto min-w-24" data-testid="instance-editor-done" on:click={() => (showEditor = false)}>Done</button>
		</footer>
	</div>
</div>
