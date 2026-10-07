export type GitHubPlugin = {
	name: string;
	author: string;
	repository: string;
	download_url: string | undefined;
};

export type ReleaseAsset = { name: string; browser_download_url: string };

export function getFetch(): typeof fetch {
	// @ts-expect-error fetchNative is injected by the Tauri shell.
	return window.fetchNative ?? window.fetch;
}

export function isInstallableAsset(asset: { name: string }): boolean {
	const name = asset.name.toLowerCase();
	return name.endsWith(".streamdeckplugin") || name.endsWith(".zip");
}

export function releasesEndpoint(repository: string): URL {
	const endpoint = new URL(repository);
	endpoint.hostname = "api." + endpoint.hostname;
	endpoint.pathname = "/repos" + endpoint.pathname + "/releases";
	return endpoint;
}

export function sortInstalledPlugins<T extends { id: string; builtin?: boolean }>(plugins: T[]): T[] {
	return plugins.sort((a, b) => (a.builtin && !b.builtin ? -1 : b.builtin && !a.builtin ? 1 : a.id.localeCompare(b.id)));
}

export function matchesQuery(name: string, query: string): boolean {
	return name.toLowerCase().includes(query.toLowerCase());
}
