import { expect, test } from "@playwright/test";

import { freshImport } from "./helpers/fresh";

type FakeEl = {
	textContent: string;
	classes: string[];
	attrs: Record<string, string>;
	removed: boolean;
	classList: { add(c: string): void };
	setAttribute(k: string, v: string): void;
	remove(): void;
};

function fakeEl(): FakeEl {
	const el: FakeEl = {
		textContent: "",
		classes: [],
		attrs: {},
		removed: false,
		classList: { add: (c) => el.classes.push(c) },
		setAttribute: (k, v) => (el.attrs[k] = v),
		remove: () => (el.removed = true),
	};
	return el;
}

const g = globalThis as any;
let saved: { document: unknown; window: unknown };
let status: FakeEl;
let loader: FakeEl | null;
let timers: { fn: () => void; ms: number }[];

test.beforeEach(() => {
	saved = { document: g.document, window: g.window };
	status = fakeEl();
	loader = fakeEl();
	timers = [];
	g.document = {
		getElementById: (id: string) => (id == "startup-status-text" ? status : id == "startup-loader" ? loader : null),
	};
	g.window = { setTimeout: (fn: () => void, ms: number) => void timers.push({ fn, ms }) };
});

test.afterEach(() => {
	g.document = saved.document;
	g.window = saved.window;
});

test.describe("showStartupTask", () => {
	test("writes the message for a pending task", async () => {
		const { showStartupTask } = await freshImport("startup.ts");
		showStartupTask("devices");
		expect(status.textContent).toBe("Detecting connected devices…");
		showStartupTask("services");
		expect(status.textContent).toBe("Preparing application services…");
		showStartupTask("actions");
		expect(status.textContent).toBe("Loading actions and plugins…");
	});

	test("ignores tasks that are already completed", async () => {
		const { showStartupTask, completeStartupTask } = await freshImport("startup.ts");
		completeStartupTask("devices");
		status.textContent = "kept";
		showStartupTask("devices");
		expect(status.textContent).toBe("kept");
	});

	test("does nothing without a document or status element", async () => {
		const { showStartupTask } = await freshImport("startup.ts");
		g.document = undefined;
		expect(() => showStartupTask("services")).not.toThrow();
		g.document = { getElementById: () => null };
		expect(() => showStartupTask("services")).not.toThrow();
	});
});

test.describe("completeStartupTask", () => {
	test("shows the next pending task while others remain", async () => {
		const { completeStartupTask } = await freshImport("startup.ts");
		completeStartupTask("services");
		expect(status.textContent).toBe("Detecting connected devices…");
		completeStartupTask("devices");
		expect(status.textContent).toBe("Loading actions and plugins…");
		expect(timers).toHaveLength(0);
	});

	test("completing the same task twice is harmless", async () => {
		const { completeStartupTask } = await freshImport("startup.ts");
		completeStartupTask("services");
		completeStartupTask("services");
		expect(timers).toHaveLength(0);
	});

	test("finishing the last task shows 'Finalizing workspace…' and schedules the loader removal", async () => {
		const { completeStartupTask } = await freshImport("startup.ts");
		completeStartupTask("services");
		completeStartupTask("devices");
		completeStartupTask("actions");
		expect(status.textContent).toBe("Finalizing workspace…");
		expect(timers).toHaveLength(1);
		expect(timers[0].ms).toBeGreaterThan(0);
		expect(timers[0].ms).toBeLessThanOrEqual(700);

		timers[0].fn();
		expect(loader!.classes).toContain("startup-loader--leaving");
		expect(loader!.attrs["aria-hidden"]).toBe("true");
		expect(timers).toHaveLength(2);
		expect(timers[1].ms).toBe(220);
		timers[1].fn();
		expect(loader!.removed).toBe(true);
	});

	test("finishing happens only once", async () => {
		const { completeStartupTask } = await freshImport("startup.ts");
		for (const task of ["services", "devices", "actions", "actions", "services"] as const) completeStartupTask(task);
		expect(timers).toHaveLength(1);
	});

	test("a missing loader element is tolerated", async () => {
		const { completeStartupTask } = await freshImport("startup.ts");
		loader = null;
		completeStartupTask("services");
		completeStartupTask("devices");
		completeStartupTask("actions");
		expect(() => timers[0].fn()).not.toThrow();
		expect(timers).toHaveLength(1);
	});

	test("the delay is 0 when the minimum visible time already elapsed", async () => {
		const realNow = Date.now;
		const { completeStartupTask } = await freshImport("startup.ts");
		Date.now = () => realNow() + 10_000;
		try {
			completeStartupTask("services");
			completeStartupTask("devices");
			completeStartupTask("actions");
		} finally {
			Date.now = realNow;
		}
		expect(timers[0].ms).toBe(0);
	});
});
