import { invoke } from "@tauri-apps/api/core";

let portBase = 57116;
let initialisation: Promise<void> | undefined;

export async function initPortBase(): Promise<void> {
	if (!initialisation) {
		initialisation = invoke<number>("get_port_base")
			.then((value) => {
				portBase = value;
			})
			.catch((error) => {
				initialisation = undefined;
				throw error;
			});
	}
	return initialisation;
}

export function getWebSocketPort(): number {
	return portBase;
}

export function getWebserverUrl(path: string = ""): string {
	return `http://127.0.0.1:${portBase + 2}/${path}`;
}
