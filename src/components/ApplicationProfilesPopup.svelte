<script lang="ts">
	import type { DeviceInfo } from "$lib/DeviceInfo";

	import Popup from "./Popup.svelte";
	import PopupHeader from "./PopupHeader.svelte";
	import ProfileOptions from "./ProfileOptions.svelte";

	import { DEFAULT_APPLICATION, isUnmapped, sortApplicationEntries, type ApplicationProfiles } from "$lib/applicationProfiles";
	import type { ProfileFolders } from "$lib/profileFolders";

	export let show: boolean;
	export let device: DeviceInfo;
	export let folders: ProfileFolders;
	export let applications: string[];
	export let applicationProfiles: ApplicationProfiles;
	export let error: string;
	export let onClose: () => void;

	const SELECT_APPLICATION = "opendeck_select_application";
	const SELECT_PROFILE = "opendeck_select_profile";

	let addAppName: string = SELECT_APPLICATION;
	let addProfile: string = SELECT_PROFILE;
	$: {
		if (addAppName != SELECT_APPLICATION && addProfile != SELECT_PROFILE) {
			applicationProfiles = {
				...applicationProfiles,
				[addAppName]: {
					...applicationProfiles[addAppName],
					[device.id]: addProfile,
				},
			};
			addAppName = SELECT_APPLICATION;
			addProfile = SELECT_PROFILE;
		}
	}

	$: unmappedApplications = applications.filter((appName) => isUnmapped(applicationProfiles, appName, device.id));
</script>

<Popup {show} testid="application-profiles-popup">
	<PopupHeader eyebrow="Application mapping" title={device.name} closeLabel="Close application mapping" {onClose} variant="compact" testid="application-profiles-header" />
	<div role="alert" class="alert mb-3">
		<span>If an application is missing, switch to it and back. The previous profile is restored when a mapped application becomes inactive.</span>
	</div>
	{#if error}
		<div role="alert" class="alert alert-error mb-3" data-testid="application-profiles-error"><span>{error}</span></div>
	{/if}

	<div class="overflow-x-auto rounded-box border border-base-300">
		<table class="table table-sm w-full">
			<thead>
				<tr><th>Application</th><th>Profile</th></tr>
			</thead>
			<tbody>
				{#each sortApplicationEntries(applicationProfiles) as [appName, devices]}
					{#if devices[device.id]}
						<tr data-testid="application-profile-row" data-application={appName}>
							<td>{appName == DEFAULT_APPLICATION ? "Default profile" : appName}:</td>
							<td>
								<select bind:value={applicationProfiles[appName][device.id]} class="select select-sm w-full" data-testid="application-profile-select">
									<ProfileOptions {folders} />
									<option disabled>──────────</option>
									<option value={undefined}>Remove application</option>
								</select>
							</td>
						</tr>
					{/if}
				{/each}
				<tr class="h-12">
					<td class="w-48">
						<select bind:value={addAppName} class="select select-sm w-full" data-testid="application-add-app">
							<option selected disabled value={SELECT_APPLICATION}>Select application...</option>
							{#if isUnmapped(applicationProfiles, DEFAULT_APPLICATION, device.id)}
								<option value={DEFAULT_APPLICATION}>Default profile</option>
								{#if unmappedApplications.length > 0}
									<option disabled>──────────</option>
								{/if}
							{/if}
							{#each unmappedApplications as appName}
								<option value={appName}>{appName}</option>
							{/each}
						</select>
					</td>
					<td class="w-96">
						<select bind:value={addProfile} class="select select-sm w-full" data-testid="application-add-profile">
							<option selected disabled value={SELECT_PROFILE}>Select profile...</option>
							<ProfileOptions {folders} />
						</select>
					</td>
				</tr>
			</tbody>
		</table>
	</div>
</Popup>
