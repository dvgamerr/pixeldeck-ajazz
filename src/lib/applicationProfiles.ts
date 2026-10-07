export type ApplicationProfiles = { [appName: string]: { [device: string]: string } };

export const DEFAULT_APPLICATION = "opendeck_default";

// Drop empty profile assignments and applications that no longer map any device.
export function cleanApplicationProfiles(value: ApplicationProfiles): ApplicationProfiles {
	return Object.fromEntries(
		Object.entries(value)
			.map(([appName, devices]) => [appName, Object.fromEntries(Object.entries(devices).filter(([_, profile]) => profile))])
			.filter(([_, devices]) => Object.keys(devices).length),
	);
}

// The default profile always sorts first, everything else alphabetically.
export function sortApplicationEntries(profiles: ApplicationProfiles): [string, { [device: string]: string }][] {
	return Object.entries(profiles).sort((a, b) => (a[0] == DEFAULT_APPLICATION ? -1 : b[0] == DEFAULT_APPLICATION ? 1 : a[0].localeCompare(b[0])));
}

export function isUnmapped(profiles: ApplicationProfiles, appName: string, deviceId: string): boolean {
	return !profiles[appName] || !profiles[appName][deviceId];
}
