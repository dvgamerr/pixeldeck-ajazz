import { expect, test as base, type Page } from "@playwright/test";

export type MockCall = { cmd: string; args: any };

export type MockOptions = {
	/** Connected devices, keyed by id. Defaults to a single Ajazz AKP05E_552A (`sd-TEST`). */
	devices?: Record<string, any>;
	/** Profile names per device id. Defaults to ["Default", "Gaming"]. */
	profiles?: Record<string, string[]>;
	settings?: Record<string, any>;
	plugins?: any[];
	categories?: Record<string, any>;
	applications?: string[];
	/** Pre-populated slots on the first profile of the first device. */
	seed?: { controller: "Keypad" | "Encoder"; position: number; action: any }[];
};

export const AKP05_ID = "sd-TEST";
export const OTHER_ID = "sd-OTHER";

export const akp05Device = (id = AKP05_ID) => ({ id, name: "Ajazz AKP05E_552A", rows: 2, columns: 5, encoders: 4, type: 7 });

export const makeAction = (uuid: string, name: string, controllers: string[], plugin = "test.plugin") => ({
	name,
	uuid,
	plugin,
	tooltip: `${name} tooltip`,
	icon: "icon.png",
	visible_in_action_list: true,
	supported_in_multi_actions: true,
	property_inspector: "",
	controllers,
	states: [{ image: "", name: "", text: "", show: false, colour: "#ffffff", alignment: "middle", family: "Arial", style: "Regular", size: 16, underline: false }],
});

export const KEY_ACTION = { ...makeAction("test.plugin.key", "Key Only Action", ["Keypad"]), property_inspector: "pi/key.html" };
export const DIAL_ACTION = makeAction("test.plugin.dial", "Dial Only Action", ["Encoder"]);
export const BOTH_ACTION = makeAction("test.plugin.both", "Hybrid Action", ["Keypad", "Encoder"]);

const defaultCategories = () => ({
	"Test Plugin": { actions: [KEY_ACTION, DIAL_ACTION, BOTH_ACTION] },
	Utilities: { actions: [makeAction("test.util.clock", "Clock", ["Keypad"], "test.util"), makeAction("test.util.volume", "Volume", ["Encoder"], "test.util")] },
});

const defaultPlugins = () => [
	{ id: "test.plugin", name: "Test Plugin", version: "1.0.0", icon: "plugin.png", registered: true, builtin: true, has_settings_interface: false },
	{ id: "test.util", name: "Utility Pack", version: "2.3.1", icon: "util.png", registered: true, builtin: false, has_settings_interface: true },
	{ id: "test.broken", name: "Broken Plugin", version: "0.0.1", icon: "broken.png", registered: false, builtin: false, has_settings_interface: false },
];

const defaultSettings = () => ({
	version: "0.1.0",
	language: "en",
	brightness: 50,
	darktheme: true,
	background: false,
	autolaunch: false,
	updatecheck: true,
	statistics: false,
	separatewine: false,
	developer: false,
	disabledevices: false,
});

/**
 * Runs inside the page (via addInitScript). It must be self-contained: no references to
 * module scope. Installs window.__TAURI_INTERNALS__ plus a `window.__mock` control surface.
 */
function installTauriMock(opts: Required<MockOptions>) {
	const w = window as any;
	const calls: MockCall[] = [];
	const unmatched: MockCall[] = [];
	const callbacks = new Map<number, (data: any) => void>();
	const listeners = new Map<number, { event: string; handler: number }>();
	let nextCallback = 1;
	let nextEventId = 1;
	let nextInstance = 1;
	// Responses for a command can be held back until the test releases them.
	const holds: { cmd: string; match: (args: any) => boolean; resolvers: (() => void)[]; active: boolean }[] = [];

	const state = {
		devices: JSON.parse(JSON.stringify(opts.devices)) as Record<string, any>,
		profileNames: JSON.parse(JSON.stringify(opts.profiles)) as Record<string, string[]>,
		selected: {} as Record<string, string>,
		profiles: {} as Record<string, any>,
		settings: JSON.parse(JSON.stringify(opts.settings)),
		plugins: JSON.parse(JSON.stringify(opts.plugins)),
		categories: JSON.parse(JSON.stringify(opts.categories)),
		applications: [...opts.applications],
		applicationProfiles: {} as Record<string, any>,
		renameError: null as string | null,
	};

	const blankProfile = (device: string, id: string) => {
		const info = state.devices[device] ?? { rows: 2, columns: 5, encoders: 4 };
		return { device, id, keys: Array(info.rows * info.columns).fill(null), sliders: Array(info.encoders).fill(null) };
	};
	const profileFor = (device: string, id: string) => {
		const key = `${device}/${id}`;
		return (state.profiles[key] ??= blankProfile(device, id));
	};
	const selectedProfile = (device: string) => {
		state.selected[device] ??= state.profileNames[device]?.[0] ?? "Default";
		return JSON.parse(JSON.stringify(profileFor(device, state.selected[device])));
	};
	const parseContext = (context: string) => {
		// device.profile.controller.position.index
		const parts = context.split(".");
		return { device: parts[0], profile: parts[1], controller: parts[2], position: Number(parts[3]) };
	};
	const makeInstance = (context: any, action: any) => ({
		action,
		context: `${context.device}.${context.profile}.${context.controller}.${context.position}.0`,
		states: JSON.parse(JSON.stringify(action.states)),
		current_state: 0,
		settings: {},
		children: null,
		_n: nextInstance++,
	});
	const slotArray = (profile: any, controller: string) => (controller == "Encoder" ? profile.sliders : profile.keys);

	const firstDevice = Object.keys(state.devices)[0];
	for (const slot of opts.seed) {
		const profileId = state.profileNames[firstDevice]?.[0] ?? "Default";
		const instance = makeInstance({ device: firstDevice, profile: profileId, controller: slot.controller, position: slot.position }, slot.action);
		slotArray(profileFor(firstDevice, profileId), slot.controller)[slot.position] = instance;
	}

	const handlers: Record<string, (args: any) => any> = {
		get_port_base: () => 57116,
		get_build_info: () => "linux x86_64 e2e-mock",
		get_settings: () => JSON.parse(JSON.stringify(state.settings)),
		set_settings: ({ settings }) => {
			state.settings = JSON.parse(JSON.stringify(settings));
			return null;
		},
		get_localisations: () => ({}),
		get_devices: () => JSON.parse(JSON.stringify(state.devices)),
		get_profiles: ({ device }) => [...(state.profileNames[device] ?? [])],
		get_selected_profile: ({ device }) => selectedProfile(device),
		reload_selected_profile: ({ device }) => selectedProfile(device),
		set_selected_profile: ({ device, id }) => {
			const names = (state.profileNames[device] ??= []);
			if (!names.includes(id)) names.push(id);
			state.selected[device] = id;
			return null;
		},
		delete_profile: ({ device, profile }) => {
			state.profileNames[device] = (state.profileNames[device] ?? []).filter((name) => name != profile);
			delete state.profiles[`${device}/${profile}`];
			return null;
		},
		rename_profile: ({ device, profile, newId }) => {
			if (state.renameError) throw state.renameError;
			const names = state.profileNames[device] ?? [];
			state.profileNames[device] = names.map((name) => (name == profile ? newId : name));
			const old = profileFor(device, profile);
			delete state.profiles[`${device}/${profile}`];
			state.profiles[`${device}/${newId}`] = { ...old, id: newId };
			if (state.selected[device] == profile) state.selected[device] = newId;
			return JSON.parse(JSON.stringify(state.profiles[`${device}/${newId}`]));
		},
		get_applications: () => [...state.applications],
		get_application_profiles: () => JSON.parse(JSON.stringify(state.applicationProfiles)),
		set_application_profiles: ({ value }) => {
			state.applicationProfiles = value;
			return null;
		},
		get_categories: () => JSON.parse(JSON.stringify(state.categories)),
		list_plugins: () => JSON.parse(JSON.stringify(state.plugins)),
		create_instance: ({ context, action }) => {
			const instance = makeInstance(context, action);
			slotArray(profileFor(context.device, context.profile), context.controller)[context.position] = instance;
			return JSON.parse(JSON.stringify(instance));
		},
		remove_instance: ({ context }) => {
			const parsed = parseContext(context);
			slotArray(profileFor(parsed.device, parsed.profile), parsed.controller)[parsed.position] = null;
			return null;
		},
		move_instance: ({ source, destination, retain }) => {
			const from = slotArray(profileFor(source.device, source.profile), source.controller);
			const to = slotArray(profileFor(destination.device, destination.profile), destination.controller);
			if (source.controller != destination.controller) throw "Cannot move between controller types";
			const original = from[source.position];
			if (!original) return null;
			const moved = { ...JSON.parse(JSON.stringify(original)), context: `${destination.device}.${destination.profile}.${destination.controller}.${destination.position}.0` };
			to[destination.position] = moved;
			if (!retain) from[source.position] = null;
			return JSON.parse(JSON.stringify(moved));
		},
		set_state: () => null,
		update_images: () => null,
		switch_property_inspector: () => null,
		make_info: () => ({ application: { version: "0.1.0" }, plugin: { version: "1.0.0" }, devices: [], colors: {} }),
		open_url: () => null,
		open_config_directory: () => null,
		open_log_directory: () => null,
		reload_plugin: () => null,
		remove_plugin: ({ id }) => {
			state.plugins = state.plugins.filter((plugin: any) => plugin.id != id);
			return null;
		},
		install_plugin: () => null,
		show_settings_interface: () => null,
		restart: () => null,
		get_startup_image_project: () => null,
		save_startup_image_project: () => null,
		set_startup_image: () => null,
		// Tauri plugins used by the frontend.
		"plugin:event|listen": ({ event, handler }) => {
			const id = nextEventId++;
			listeners.set(id, { event, handler });
			return id;
		},
		"plugin:event|unlisten": ({ eventId }) => {
			listeners.delete(eventId);
			return null;
		},
		"plugin:event|emit": () => null,
		"plugin:window|set_min_size": () => null,
		"plugin:window|set_size": () => null,
		"plugin:window|inner_size": () => ({ width: 1440, height: 900 }),
		"plugin:deep-link|get_current": () => null,
		"plugin:dialog|ask": () => true,
		"plugin:dialog|message": ({ buttons }) => (buttons === "YesNo" ? "Yes" : "Ok"),
		"plugin:dialog|open": () => null,
	};

	const mock = {
		calls,
		unmatched,
		state,
		/** Calls recorded for one command. */
		callsTo: (cmd: string) => calls.filter((call) => call.cmd == cmd),
		/** Deliver an event to every page listener registered for it. */
		emit: (event: string, payload: unknown) => {
			for (const [id, listener] of [...listeners]) {
				if (listener.event != event) continue;
				callbacks.get(listener.handler)?.({ event, id, payload });
			}
		},
		listenerCount: (event: string) => [...listeners.values()].filter((l) => l.event == event).length,
		/** Hold responses to `cmd` (optionally only when `match(args)` is truthy) until `release` is called. */
		hold: (cmd: string, matchSource?: string) => {
			const match = matchSource ? (new Function("args", `return (${matchSource})(args);`) as (args: any) => boolean) : () => true;
			holds.push({ cmd, match, resolvers: [], active: true });
		},
		release: (cmd: string) => {
			for (const hold of holds) {
				if (hold.cmd != cmd) continue;
				hold.active = false;
				hold.resolvers.splice(0).forEach((resolve) => resolve());
			}
		},
		setRenameError: (message: string | null) => (state.renameError = message),
		clearUnmatched: () => unmatched.splice(0),
	};
	w.__mock = mock;

	w.__TAURI_INTERNALS__ = {
		metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
		transformCallback: (callback: (data: any) => void, once = false) => {
			const id = nextCallback++;
			callbacks.set(id, (data) => {
				if (once) callbacks.delete(id);
				callback(data);
			});
			return id;
		},
		unregisterCallback: (id: number) => callbacks.delete(id),
		convertFileSrc: (path: string) => path,
		invoke: async (cmd: string, args: any = {}) => {
			const record = { cmd, args: JSON.parse(JSON.stringify(args ?? {})) };
			if (!cmd.startsWith("plugin:event|")) calls.push(record);
			const handler = handlers[cmd];
			if (!handler) {
				unmatched.push(record);
				console.error(`[tauri-mock] unmatched invoke: ${cmd}`);
				return null;
			}
			for (const hold of holds) {
				if (hold.active && hold.cmd == cmd && hold.match(args)) await new Promise<void>((resolve) => hold.resolvers.push(resolve));
			}
			return handler(args);
		},
	};
	w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
}

const PNG = Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/q842iQAAAABJRU5ErkJggg==", "base64");

/** Typed wrapper around the in-page `window.__mock` surface. */
export class TauriMock {
	constructor(private page: Page) {}

	calls(cmd?: string): Promise<MockCall[]> {
		return this.page.evaluate((name) => ((window as any).__mock.calls as MockCall[]).filter((call) => !name || call.cmd == name), cmd);
	}
	unmatched(): Promise<MockCall[]> {
		return this.page.evaluate(() => (window as any).__mock.unmatched);
	}
	emit(event: string, payload: unknown): Promise<void> {
		return this.page.evaluate(([name, data]) => (window as any).__mock.emit(name, data), [event, payload] as const);
	}
	listenerCount(event: string): Promise<number> {
		return this.page.evaluate((name) => (window as any).__mock.listenerCount(name), event);
	}
	/** Hold responses until release(); `match` is a function expression string evaluated in the page, e.g. `"(a) => a.device == 'sd-X'"`. */
	hold(cmd: string, match?: string): Promise<void> {
		return this.page.evaluate(([name, source]) => (window as any).__mock.hold(name, source), [cmd, match] as const);
	}
	release(cmd: string): Promise<void> {
		return this.page.evaluate((name) => (window as any).__mock.release(name), cmd);
	}
	mockState<T = any>(read: string): Promise<T> {
		return this.page.evaluate((source) => new Function("state", `return (${source})(state);`)((window as any).__mock.state), read);
	}
	clearUnmatched(): Promise<void> {
		return this.page.evaluate(() => void (window as any).__mock.clearUnmatched());
	}
	setRenameError(message: string | null): Promise<void> {
		return this.page.evaluate((m) => (window as any).__mock.setRenameError(m), message);
	}
}

export const test = base.extend<{ options: MockOptions; tauri: TauriMock }>({
	options: [{}, { option: true }],
	tauri: [
		async ({ page, options }, use) => {
			const resolved: Required<MockOptions> = {
				devices: options.devices ?? { [AKP05_ID]: akp05Device() },
				profiles: options.profiles ?? Object.fromEntries(Object.keys(options.devices ?? { [AKP05_ID]: 1 }).map((id) => [id, ["Default", "Gaming"]])),
				settings: { ...defaultSettings(), ...options.settings },
				plugins: options.plugins ?? defaultPlugins(),
				categories: options.categories ?? defaultCategories(),
				applications: options.applications ?? ["firefox", "code"],
				seed: options.seed ?? [],
			};
			await page.addInitScript(installTauriMock, resolved);

			// No network: stub the local plugin webserver (images) and the remote plugin catalogues.
			await page.route("http://127.0.0.1:*/**", (route) => route.fulfill({ status: 200, contentType: "image/png", body: PNG }));
			await page.route("https://openactionapi.github.io/**", (route) =>
				route.request().url().endsWith("catalogue.json")
					? route.fulfill({
							json: {
								"com.example.store-one": { name: "Store Alpha", author: "Alice", repository: "https://github.com/alice/alpha", download_url: "https://example.invalid/alpha.zip" },
								"com.example.store-two": { name: "Store Beta", author: "Bob", repository: "https://github.com/bob/beta" },
							},
						})
					: route.fulfill({ status: 200, contentType: "image/png", body: PNG }),
			);
			await page.route("https://plugins.amankhanna.me/**", (route) =>
				route.request().url().endsWith("catalogue.json")
					? route.fulfill({ json: [{ id: "archive.gamma", name: "Archive Gamma", author: "Carol" }] })
					: route.fulfill({ status: 200, contentType: "image/png", body: PNG }),
			);

			const mock = new TauriMock(page);
			await use(mock);
			// Unmatched-invoke guard: every spec fails if the app called a command the mock does not know.
			expect(await mock.unmatched().catch(() => []), "invoke commands without a mock handler").toEqual([]);
		},
		{ auto: true },
	],
});

export { expect };

/** Open the app and wait until the AKP05E_552A workspace is rendered. */
export async function openApp(page: Page) {
	await page.goto("/");
	await page.getByTestId("device-view").waitFor();
	// Let fonts and the preview-scale ResizeObserver settle so geometry-based assertions are stable.
	await page.evaluate(async () => {
		await document.fonts.ready;
		await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
	});
}

/** Build a DataTransfer in the page and return a handle usable with locator.dispatchEvent. */
export async function makeDataTransfer(page: Page, data: Record<string, string>) {
	return page.evaluateHandle((entries) => {
		const transfer = new DataTransfer();
		for (const [type, value] of Object.entries(entries)) transfer.setData(type, value);
		return transfer;
	}, data);
}

export const ACTION_MIME = "application/x-opendeck-action";

/** Simulate dropping a library action onto a key/zone (dragover then drop on its canvas). */
export async function dropActionOn(page: Page, testId: string, action: unknown, mimes: string[] = [ACTION_MIME, "action"]) {
	const serialized = JSON.stringify(action);
	const transfer = await makeDataTransfer(page, Object.fromEntries(mimes.map((mime) => [mime, serialized])));
	const canvas = page.getByTestId(testId).getByTestId("key-canvas");
	await canvas.dispatchEvent("dragover", { dataTransfer: transfer });
	await canvas.dispatchEvent("drop", { dataTransfer: transfer });
}

/** Dispatch a real MouseEvent("contextmenu") at explicit viewport coordinates (even outside the element). */
export async function contextMenuAt(page: Page, testId: string, clientX: number, clientY: number) {
	await page
		.getByTestId(testId)
		.getByTestId("key-canvas")
		.evaluate((node, point) => node.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: point.x, clientY: point.y })), { x: clientX, y: clientY });
}
