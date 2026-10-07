import { expect, test } from "@playwright/test";

import type { Action } from "$lib/Action";
import type { ActionInstance } from "$lib/ActionInstance";
import type { ActionState } from "$lib/ActionState";
import type { DeviceInfo } from "$lib/DeviceInfo";
import type { Profile } from "$lib/Profile";
import type { ApplicationProfiles } from "$lib/applicationProfiles";
import type { ProfileFolders } from "$lib/profileFolders";
import type { GitHubPlugin, ReleaseAsset } from "$lib/plugins";
import type { StartupTask } from "$lib/startup";
import type { Settings } from "$lib/settings";

const state = {
	image: "a.png",
	name: "On",
	text: "hi",
	show: true,
	colour: "#fff",
	alignment: "middle",
	family: "Arial",
	style: "Regular",
	size: 12,
	underline: false,
} satisfies ActionState;

const action = {
	name: "Act",
	uuid: "com.x.act",
	plugin: "com.x",
	tooltip: "t",
	icon: "i.png",
	visible_in_action_list: true,
	supported_in_multi_actions: false,
	property_inspector: "pi.html",
	controllers: ["Keypad", "Encoder"],
	states: [state],
} satisfies Action;

test.describe("type-only modules (Action, ActionInstance, ActionState, DeviceInfo, Profile)", () => {
	test("Action / ActionState / ActionInstance compose into a nested instance tree", () => {
		const child = { action, context: "c2", states: [state], current_state: 0, settings: {}, children: null } satisfies ActionInstance;
		const parent = { action, context: "c1", states: [state], current_state: 1, settings: { a: 1 }, children: [child] } satisfies ActionInstance;
		expect(parent.children?.[0].context).toBe("c2");
		expect(parent.action.controllers).toContain("Encoder");
		expect(["top", "middle", "bottom"]).toContain(parent.states[0].alignment);
	});

	test("Profile holds Keypad keys and Encoder sliders with nullable slots", () => {
		const profile = { device: "sd-1", id: "Default", keys: [null, null], sliders: [null] } satisfies Profile;
		expect(profile.keys).toHaveLength(2);
		expect(profile.sliders.every((slot) => slot === null)).toBe(true);
	});

	test("DeviceInfo startup_image is optional", () => {
		const base = { id: "sd-1", name: "Ajazz AKP05E_552A", rows: 2, columns: 5, encoders: 4, type: 7 } satisfies DeviceInfo;
		const withImage = { ...base, startup_image: { width: 810, height: 470 } } satisfies DeviceInfo;
		expect((base as DeviceInfo).startup_image).toBeUndefined();
		expect(withImage.startup_image.width).toBe(810);
	});

	test("the type-only modules export no runtime values", async () => {
		for (const name of ["Action", "ActionInstance", "ActionState", "DeviceInfo", "Profile"]) {
			const mod = await import(`../src/lib/${name}.ts`);
			expect(Object.keys(mod), name).toEqual([]);
		}
	});

	test("type exports of mixed modules compile (ApplicationProfiles, ProfileFolders, GitHubPlugin, ReleaseAsset, StartupTask, Settings)", () => {
		const apps: ApplicationProfiles = { app: { dev: "p" } };
		const folders: ProfileFolders = { "": ["Default"] };
		const gh: GitHubPlugin = { name: "n", author: "a", repository: "https://github.com/a/n", download_url: undefined };
		const asset: ReleaseAsset = { name: "x.zip", browser_download_url: "https://x" };
		const task: StartupTask = "services";
		const s: Partial<Settings> = { brightness: 50 };
		expect([apps, folders, gh, asset, task, s]).toHaveLength(6);
	});
});
