<script lang="ts">
	import type { DeviceInfo } from "$lib/DeviceInfo";
	import type { Profile } from "$lib/Profile";

	import Browsers from "phosphor-svelte/lib/Browsers";
	import Check from "phosphor-svelte/lib/Check";
	import Pencil from "phosphor-svelte/lib/Pencil";
	import Trash from "phosphor-svelte/lib/Trash";
	import X from "phosphor-svelte/lib/X";
	import ApplicationProfilesPopup from "./ApplicationProfilesPopup.svelte";
	import Popup from "./Popup.svelte";
	import PopupHeader from "./PopupHeader.svelte";
	import ProfileOptions from "./ProfileOptions.svelte";

	import { invoke } from "@tauri-apps/api/core";
	import { listen, type UnlistenFn } from "@tauri-apps/api/event";
	import { onMount } from "svelte";
	import { cleanApplicationProfiles, type ApplicationProfiles } from "$lib/applicationProfiles";
	import { addToFolders, makeFolders, type ProfileFolders } from "$lib/profileFolders";
	import { copiedContext, inspectedInstance, inspectedParentAction, openContextMenu } from "$lib/propertyInspector";

	let folders: ProfileFolders = {};
	let value: string;
	let disposed = false;
	let profileRequest = 0;
	let profileManagerError = "";
	let renamingProfile = "";
	let renameValue = "";

	async function getProfiles(device: DeviceInfo) {
		const request = ++profileRequest;
		const deviceId = device.id;
		try {
			const [profiles, selected] = await Promise.all([invoke<string[]>("get_profiles", { device: deviceId }), invoke<Profile>("get_selected_profile", { device: deviceId })]);
			if (disposed || request != profileRequest || device.id != deviceId) return;

			folders = makeFolders(profiles);
			profile = selected;
			value = profile.id;
			oldValue = value;
		} catch {
			// The selected device may disconnect while profiles are loading.
		}
	}

	export let device: DeviceInfo;

	export let profile: Profile;
	export function applySelectedProfile(selected: Profile) {
		profile = selected;
		value = selected.id;
		oldValue = selected.id;

		addToFolders(folders, selected.id);
		folders = folders;
	}

	export async function setProfile(id: string) {
		if (!device || !id) return;
		if (value != id) {
			value = id;
			return;
		}
		const deviceId = device.id;
		try {
			await invoke("set_selected_profile", { device: deviceId, id });
			const selected: Profile = await invoke("get_selected_profile", { device: deviceId });
			if (disposed || device.id != deviceId) return;
			profile = selected;
		} catch {
			return;
		}

		addToFolders(folders, id);
		folders = folders;
	}

	async function deleteProfile(id: string) {
		profileManagerError = "";
		try {
			await invoke("delete_profile", { device: device.id, profile: id });
			for (const devices of Object.values(applicationProfiles)) {
				if (devices[device.id] == id) delete devices[device.id];
			}
			applicationProfiles = cleanApplicationProfiles(applicationProfiles);
			lastSavedApplicationProfiles = JSON.stringify(applicationProfiles);
			await getProfiles(device);
		} catch (error) {
			profileManagerError = `Unable to delete profile: ${String(error)}`;
		}
	}

	function beginRename(id: string) {
		renamingProfile = id;
		renameValue = id;
		profileManagerError = "";
	}

	async function renameProfile(id: string) {
		if (!renameValue || renameValue == id) {
			renamingProfile = "";
			return;
		}
		if (!/^[a-zA-Z0-9_ ]+(\/[a-zA-Z0-9_ ]+)?$/.test(renameValue)) {
			profileManagerError = "Profile names may contain letters, numbers, spaces, underscores, and one folder separator.";
			return;
		}
		profileManagerError = "";
		try {
			const selected = await invoke<Profile>("rename_profile", { device: device.id, profile: id, newId: renameValue });
			inspectedInstance.set(null);
			inspectedParentAction.set(null);
			openContextMenu.set(null);
			copiedContext.set(null);
			profile = selected;
			value = selected.id;
			oldValue = selected.id;
			renamingProfile = "";
			const loadedProfiles = await invoke<ApplicationProfiles>("get_application_profiles");
			applicationProfiles = cleanApplicationProfiles(loadedProfiles);
			lastSavedApplicationProfiles = JSON.stringify(applicationProfiles);
			await getProfiles(device);
		} catch (error) {
			profileManagerError = `Unable to rename profile: ${String(error)}`;
		}
	}

	let oldValue: string;
	$: {
		if (value == "opendeck_edit_profiles") {
			if (oldValue) showPopup = true;
			value = oldValue;
		} else if (value && value != oldValue && (!profile || profile.id != value)) {
			setProfile(value);
			oldValue = value;
		}
	}

	let showPopup: boolean = false;
	let nameInput: HTMLInputElement;

	let showApplicationManager: boolean = false;
	let applications: string[] = [];
	let applicationProfiles: ApplicationProfiles = {};
	let applicationProfilesLoaded = false;
	let lastSavedApplicationProfiles = "";
	let applicationProfilesSaving = false;
	let pendingApplicationProfiles: ApplicationProfiles | undefined;
	let applicationProfilesError = "";

	async function persistApplicationProfiles(value: ApplicationProfiles) {
		pendingApplicationProfiles = cleanApplicationProfiles(value);
		if (applicationProfilesSaving) return;

		applicationProfilesSaving = true;
		try {
			while (pendingApplicationProfiles) {
				const next = pendingApplicationProfiles;
				pendingApplicationProfiles = undefined;
				const serialized = JSON.stringify(next);
				if (serialized == lastSavedApplicationProfiles) continue;

				try {
					await invoke("set_application_profiles", { value: next });
					lastSavedApplicationProfiles = serialized;
					applicationProfilesError = "";
				} catch (error) {
					applicationProfilesError = `Unable to save application profiles: ${String(error)}`;
					console.error(applicationProfilesError);
					break;
				}
			}
		} finally {
			applicationProfilesSaving = false;
		}
	}

	onMount(() => {
		const unlisteners: UnlistenFn[] = [];
		const keep = (promise: Promise<UnlistenFn>) => {
			void promise.then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
		};

		void getProfiles(device);
		void Promise.all([invoke<string[]>("get_applications"), invoke<ApplicationProfiles>("get_application_profiles")])
			.then(([loadedApplications, loadedProfiles]) => {
				if (disposed) return;
				applications = loadedApplications;
				applicationProfiles = cleanApplicationProfiles(loadedProfiles);
				lastSavedApplicationProfiles = JSON.stringify(applicationProfiles);
				applicationProfilesLoaded = true;
			})
			.catch((error) => {
				applicationProfilesError = `Unable to load application profiles: ${String(error)}`;
				console.error(applicationProfilesError);
			});

		keep(
			listen("rerender_images", async () => {
				const deviceId = device.id;
				try {
					const selected: Profile = await invoke("get_selected_profile", { device: deviceId });
					if (!disposed && device.id == deviceId) profile = selected;
				} catch {
					// The device or profile can disappear while the event is in flight.
				}
			}),
		);
		keep(listen("applications", ({ payload }: { payload: string[] }) => (applications = payload)));

		return () => {
			disposed = true;
			profileRequest += 1;
			unlisteners.forEach((unlisten) => unlisten());
		};
	});

	$: {
		if (applicationProfilesLoaded) {
			const cleaned = cleanApplicationProfiles(applicationProfiles);
			const serialized = JSON.stringify(cleaned);
			if (serialized != lastSavedApplicationProfiles) {
				if (serialized != JSON.stringify(applicationProfiles)) applicationProfiles = cleaned;
				void persistApplicationProfiles(cleaned);
			}
		}
	}
</script>

<select bind:value class="select select-sm w-full" aria-label="Profile" data-testid="profile-selector">
	<ProfileOptions {folders} />
	<option value="opendeck_edit_profiles">Edit...</option>
</select>

<svelte:window
	on:keydown={(event) => {
		if (event.key == "Escape") {
			if (showApplicationManager) showApplicationManager = false;
			else showPopup = false;
		}
	}}
/>

<Popup show={showPopup} testid="profile-manager">
	<PopupHeader eyebrow="Profiles" title={device.name} closeLabel="Close profile manager" onClose={() => (showPopup = false)} variant="compact" testid="profile-manager-header" />

	<div class="join mb-3 flex w-full">
		<input
			bind:this={nameInput}
			data-testid="profile-name-input"
			pattern="[a-zA-Z0-9_ ]+(\/[a-zA-Z0-9_ ]+)?"
			class="input input-bordered join-item grow invalid:input-error"
			placeholder="Profile ID (e.g. &quot;folder/profile&quot;)"
		/>

		<button
			type="button"
			data-testid="profile-create"
			on:click={async () => {
				if (!nameInput.checkValidity() || !nameInput.value) return;
				await setProfile(nameInput.value);
				value = nameInput.value;
				nameInput.value = "";
				showPopup = false;
			}}
			class="btn btn-primary join-item"
		>
			Create
		</button>

		<button type="button" class="btn join-item" data-testid="profile-application-mapping" title="Manage application profiles" on:click={() => (showApplicationManager = true)}>
			<Browsers size={24} />
		</button>
	</div>

	{#if profileManagerError}
		<div role="alert" class="alert alert-error mb-3 py-2 text-sm" data-testid="profile-manager-error"><span>{profileManagerError}</span></div>
	{/if}

	<div class="divide-y divide-base-300 rounded-box border border-base-300 bg-base-200 px-3" data-testid="profile-list">
		{#each Object.entries(folders) as [id, profiles]}
			{#if id && profiles.length}
				<h4 class="ui-eyebrow py-2">{id}</h4>
			{/if}
			{#each profiles as profile}
				<div class="flex items-center gap-3 py-2" class:ml-6={id} class:pl-2={id} data-testid="profile-item" data-profile={profile}>
					{#if renamingProfile == profile}
						<input
							class="input input-bordered input-sm min-w-0 flex-1"
							data-testid="profile-rename-input"
							pattern="[a-zA-Z0-9_ ]+(\/[a-zA-Z0-9_ ]+)?"
							bind:value={renameValue}
							aria-label={`Rename profile ${profile}`}
							on:keydown={(event) => {
								if (event.key == "Enter") void renameProfile(profile);
								if (event.key == "Escape") renamingProfile = "";
							}}
						/>
						<button type="button" class="btn btn-circle btn-primary btn-xs" aria-label="Save profile name" data-testid="profile-rename-save" on:click={() => renameProfile(profile)}>
							<Check size="15" weight="bold" />
						</button>
						<button type="button" class="icon-btn-xs" aria-label="Cancel rename" data-testid="profile-rename-cancel" on:click={() => (renamingProfile = "")}>
							<X size="15" weight="bold" />
						</button>
					{:else}
						<label class="flex min-w-0 flex-1 cursor-pointer items-center gap-3">
							<input type="radio" bind:group={value} value={profile} class="radio radio-primary radio-sm" data-testid="profile-radio" />
							<span class="truncate">{id ? profile.split("/")[1] : profile}</span>
						</label>
						<button type="button" class="icon-btn-xs" aria-label={`Rename profile ${profile}`} data-testid="profile-rename" on:click={() => beginRename(profile)}>
							<Pencil size="16" />
						</button>
					{/if}
					{#if profile != value && renamingProfile != profile}
						<button type="button" on:click={() => deleteProfile(profile)} class="icon-btn-xs text-error" aria-label="Delete profile" data-testid="profile-delete">
							<Trash size="18" />
						</button>
					{/if}
				</div>
			{/each}
		{/each}
	</div>
</Popup>

<ApplicationProfilesPopup show={showApplicationManager} {device} {folders} {applications} bind:applicationProfiles error={applicationProfilesError} onClose={() => (showApplicationManager = false)} />
