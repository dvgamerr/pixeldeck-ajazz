export type ProfileFolders = { [name: string]: string[] };

function folderOf(id: string): string {
	return id.includes("/") ? id.split("/")[0] : "";
}

export function makeFolders(profiles: string[]): ProfileFolders {
	const folders: ProfileFolders = {};
	for (const id of profiles) addToFolders(folders, id);
	return folders;
}

// Adds a profile id to its folder (mutating), creating the folder when needed.
export function addToFolders(folders: ProfileFolders, id: string): void {
	const folder = folderOf(id);
	if (!folders[folder]) folders[folder] = [id];
	else if (!folders[folder].includes(id)) folders[folder].push(id);
}
