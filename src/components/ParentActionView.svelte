<script lang="ts">
	import type { ActionInstance } from "$lib/ActionInstance";
	import type { Profile } from "$lib/Profile";

	import ArrowLeft from "phosphor-svelte/lib/ArrowLeft";
	import ArrowsClockwise from "phosphor-svelte/lib/ArrowsClockwise";
	import Lightning from "phosphor-svelte/lib/Lightning";
	import Plus from "phosphor-svelte/lib/Plus";
	import Stack from "phosphor-svelte/lib/Stack";
	import Trash from "phosphor-svelte/lib/Trash";
	import Key from "./Key.svelte";

	import { inspectedInstance, inspectedParentAction } from "$lib/propertyInspector";

	import { invoke } from "@tauri-apps/api/core";

	export let profile: Profile;

	let children: ActionInstance[];
	$: children = profile.keys[$inspectedParentAction!.position]!.children!;
	let parentUuid: string;
	$: parentUuid = profile.keys[$inspectedParentAction!.position]!.action.uuid;
	let isToggle: boolean;
	$: isToggle = parentUuid == "opendeck.toggleaction";
	let dragActive = false;

	function closeFlow() {
		inspectedInstance.set(null);
		inspectedParentAction.set(null);
	}

	function selectInstance(instance: ActionInstance) {
		inspectedInstance.set(instance.context);
	}

	function handleDragOver(event: DragEvent) {
		event.preventDefault();
		return true;
	}

	async function handleDrop({ dataTransfer }: DragEvent) {
		dragActive = false;
		if (dataTransfer?.getData("action")) {
			let action = JSON.parse(dataTransfer?.getData("action"));
			if (
				(parentUuid == "opendeck.multiaction" && !action.supported_in_multi_actions) ||
				(parentUuid == "opendeck.toggleaction" && (action.uuid == "opendeck.multiaction" || action.uuid == "opendeck.toggleaction"))
			) {
				return;
			}
			let response: ActionInstance | null = await invoke("create_instance", { context: $inspectedParentAction, action });
			if (response) {
				profile.keys[$inspectedParentAction!.position]!.children = [...children, response];
				selectInstance(response);
			}
		}
	}

	async function removeInstance(index: number) {
		const context = children[index]?.context;
		if (!context) return;
		await invoke("remove_instance", { context });
		if ($inspectedInstance == context) inspectedInstance.set(null);
		profile.keys[$inspectedParentAction!.position]!.children = children.filter((_, childIndex) => childIndex != index);
	}
</script>

<svelte:window
	on:keydown={(event) => {
		if (event.key == "Escape") closeFlow();
	}}
/>

<section class="flex h-full min-h-0 w-full flex-col bg-base-200/40" data-testid="parent-action-view" data-parent-uuid={parentUuid}>
	<header class="flex shrink-0 items-center gap-3 border-b border-base-300 bg-base-100 px-5 py-3">
		<button type="button" class="btn btn-circle btn-ghost btn-sm" aria-label="Back to device" title="Back to device" data-testid="parent-action-back" on:click={closeFlow}>
			<ArrowLeft size="19" />
		</button>
		<div class={`flex size-10 shrink-0 items-center justify-center rounded-box ${isToggle ? "bg-secondary/10 text-secondary" : "bg-primary/10 text-primary"}`}>
			{#if isToggle}
				<ArrowsClockwise size="21" weight="bold" />
			{:else}
				<Stack size="21" weight="bold" />
			{/if}
		</div>
		<div class="min-w-0">
			<div class="flex items-center gap-2">
				<h1 data-testid="parent-action-title" class="ui-page-title truncate">{isToggle ? "Toggle Action" : "Multi Action"}</h1>
				<span data-testid="parent-action-count" class="badge badge-neutral badge-sm shrink-0">{children.length} {children.length == 1 ? "action" : "actions"}</span>
			</div>
			<p class="ui-caption ui-muted mt-0.5 truncate">
				{isToggle ? "Run one action per press, then continue to the next state." : "Run every action in sequence from top to bottom."}
			</p>
		</div>
		<div class="ml-auto hidden shrink-0 items-center gap-1.5 text-xs text-base-content/50 xl:flex">
			<span class="kbd kbd-sm">Esc</span>
			<span>Back</span>
		</div>
	</header>

	<!-- svelte-ignore a11y-no-static-element-interactions -->
	<div class="min-h-0 flex-1 overflow-y-auto" on:click={() => inspectedInstance.set(null)} on:keyup={() => inspectedInstance.set(null)}>
		<div class={`mx-auto flex w-full max-w-3xl flex-col px-6 pt-5 ${$inspectedInstance ? "pb-[25rem]" : "pb-6"}`}>
			<div class={`mb-5 flex items-start gap-3 rounded-box border px-4 py-3 ${isToggle ? "border-secondary/20 bg-secondary/5" : "border-primary/20 bg-primary/5"}`}>
				<div class={`mt-0.5 flex size-8 shrink-0 items-center justify-center rounded-full ${isToggle ? "bg-secondary/15 text-secondary" : "bg-primary/15 text-primary"}`}>
					{#if isToggle}
						<ArrowsClockwise size="17" weight="bold" />
					{:else}
						<Lightning size="17" weight="fill" />
					{/if}
				</div>
				<div>
					<p class="ui-label">{isToggle ? "Cycle on each press" : "Sequential flow"}</p>
					<p class="ui-caption ui-muted mt-0.5 leading-relaxed">
						{isToggle
							? "The first press runs State 1, the next press runs State 2, and the cycle repeats."
							: "Each press runs Step 1 through the final step in order, with a short pause between actions."}
					</p>
				</div>
			</div>

			<div class="flex flex-col" data-testid="parent-action-children">
				{#each children as instance, index}
					<div class="grid grid-cols-[2.5rem_minmax(0,1fr)] gap-3">
						<div class="flex flex-col items-center" aria-hidden="true">
							<div
								class={`z-10 flex size-8 shrink-0 items-center justify-center rounded-full border text-xs font-bold ${$inspectedInstance == instance.context ? "border-primary bg-primary text-primary-content" : "border-base-300 bg-base-100 text-base-content/60"}`}
							>
								{index + 1}
							</div>
							<div class="min-h-3 w-px flex-1 bg-base-300"></div>
						</div>

						<!-- svelte-ignore a11y-no-static-element-interactions -->
						<div
							class={`group mb-3 flex min-w-0 cursor-pointer items-center rounded-box border bg-base-100 px-3 py-2.5 transition-colors ${$inspectedInstance == instance.context ? "border-primary shadow-sm" : "border-base-300 hover:border-base-content/25 hover:bg-base-100/80"}`}
							role="button"
							data-testid="parent-action-child"
							data-context={instance.context}
							tabindex="0"
							aria-pressed={$inspectedInstance == instance.context}
							on:click|stopPropagation={() => selectInstance(instance)}
							on:keyup|stopPropagation={(event) => {
								if (event.key == "Enter" || event.key == " ") selectInstance(instance);
							}}
						>
							<div class="flex size-[4.25rem] shrink-0 items-center justify-center overflow-hidden rounded-box border border-base-300 bg-base-200">
								<Key inslot={instance} context={null} active={false} scale={1 / 2} />
							</div>
							<div class="ml-3 min-w-0 flex-1">
								<div class="flex items-center gap-2">
									<span data-testid="parent-action-child-label" class={`badge badge-sm ${isToggle ? "badge-secondary" : "badge-primary"}`}>{isToggle ? `State ${index + 1}` : `Step ${index + 1}`}</span
									>
									{#if $inspectedInstance == instance.context}
										<span class="badge badge-ghost badge-sm">Editing</span>
									{/if}
								</div>
								<p class="mt-1.5 truncate font-semibold">{instance.action.name}</p>
								<p class="ui-caption ui-muted mt-0.5 truncate">{instance.action.plugin}</p>
							</div>
							<button
								type="button"
								class="btn btn-circle btn-ghost btn-sm ml-2 shrink-0 text-base-content/45 opacity-70 hover:bg-error/10 hover:text-error group-hover:opacity-100"
								aria-label={`Remove ${instance.action.name}`}
								title="Remove action"
								data-testid="parent-action-child-remove"
								on:click|stopPropagation={() => removeInstance(index)}
							>
								<Trash size="18" />
							</button>
						</div>
					</div>
				{/each}

				<div class="grid grid-cols-[2.5rem_minmax(0,1fr)] gap-3">
					<div class="flex flex-col items-center" aria-hidden="true">
						<div
							class={`flex size-8 shrink-0 items-center justify-center rounded-full border-2 border-dashed ${dragActive ? "border-primary bg-primary text-primary-content" : "border-base-300 bg-base-100 text-base-content/45"}`}
						>
							<Plus size="16" weight="bold" />
						</div>
					</div>
					<div
						data-testid="parent-action-dropzone"
						class={`flex min-h-24 items-center rounded-box border-2 border-dashed px-4 py-3 transition-colors ${dragActive ? "border-primary bg-primary/10" : "border-base-300 bg-base-100/45 hover:border-base-content/30 hover:bg-base-100/70"}`}
						on:dragenter={() => (dragActive = true)}
						on:dragleave={(event) => {
							if (!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node | null)) dragActive = false;
						}}
						on:dragover={handleDragOver}
						on:drop={handleDrop}
					>
						<div class={`flex size-11 shrink-0 items-center justify-center rounded-box ${dragActive ? "bg-primary text-primary-content" : "bg-base-200 text-base-content/45"}`}>
							<Plus size="21" weight="bold" />
						</div>
						<div class="ml-3 min-w-0">
							<p class="font-semibold">{children.length == 0 ? "Add your first action" : `Add ${isToggle ? "another state" : "next step"}`}</p>
							<p class="ui-caption ui-muted mt-0.5">Drag an action from the library and drop it here.</p>
						</div>
					</div>
				</div>
			</div>

			<div class="mt-4 flex items-center justify-between gap-4 border-t border-base-300 pt-3 text-xs text-base-content/50">
				<span>Click an action to edit its settings.</span>
				<span class="shrink-0">{isToggle ? "Repeats after the last state" : "Runs top to bottom"}</span>
			</div>
		</div>
	</div>
</section>
