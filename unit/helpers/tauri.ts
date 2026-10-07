// Minimal stand-in for the Tauri IPC bridge so `@tauri-apps/api/core` `invoke`
// can run in plain Node. Only the pieces `invoke` touches are provided.
export type InvokeCall = { cmd: string; args: any };
export type InvokeHandler = (cmd: string, args: any) => unknown | Promise<unknown>;

export function installTauriMock(handler: InvokeHandler = () => undefined) {
	const calls: InvokeCall[] = [];
	const g = globalThis as any;
	g.window = g.window ?? globalThis;
	g.__TAURI_INTERNALS__ = {
		invoke: async (cmd: string, args: any) => {
			calls.push({ cmd, args });
			return handler(cmd, args);
		},
		transformCallback: () => 0,
	};
	return {
		calls,
		setHandler(next: InvokeHandler) {
			handler = next;
		},
	};
}

export function uninstallTauriMock() {
	delete (globalThis as any).__TAURI_INTERNALS__;
}
