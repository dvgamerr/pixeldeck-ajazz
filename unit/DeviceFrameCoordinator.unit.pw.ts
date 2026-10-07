import { expect, test } from "@playwright/test";

import type { Context } from "$lib/Context";
import { type DeviceFrame, DeviceFrameCoordinator } from "$lib/DeviceFrameCoordinator";
import { sleep } from "./helpers/fresh";

const ctx = (position: number, device = "dev", profile = "P", controller = "Keypad"): Context => ({ device, profile, controller, position });
const frame = (position: number, image: string | null = `img${position}`, device = "dev", profile = "P", controller = "Keypad"): DeviceFrame => ({
	context: ctx(position, device, profile, controller),
	image,
});

function make(options: { initial?: number; live?: number; send?: (frames: DeviceFrame[]) => Promise<void> } = {}) {
	const batches: DeviceFrame[][] = [];
	const errors: unknown[] = [];
	const coordinator = new DeviceFrameCoordinator(
		async (frames) => {
			batches.push(frames);
			await options.send?.(frames);
		},
		options.initial ?? 40,
		options.live ?? 5,
		(error) => errors.push(error),
	);
	return { coordinator, batches, errors };
}

test.describe("initial render", () => {
	test("sends one batch as soon as the expected number of frames arrived", async () => {
		const { coordinator, batches } = make({ initial: 500 });
		coordinator.beginInitialRender("dev", "P", 2);
		coordinator.queue(frame(0));
		await sleep(20);
		expect(batches).toHaveLength(0);
		coordinator.queue(frame(1));
		await coordinator.flushPending("dev");
		expect(batches).toHaveLength(1);
		expect(batches[0].map((f) => f.context.position).sort()).toEqual([0, 1]);
	});

	test("flushes partial frames when the initial timeout elapses", async () => {
		const { coordinator, batches } = make({ initial: 20 });
		coordinator.beginInitialRender("dev", "P", 5);
		coordinator.queue(frame(0));
		await sleep(80);
		expect(batches).toHaveLength(1);
		expect(batches[0]).toHaveLength(1);
	});

	test("timeout with no frames sends nothing", async () => {
		const { coordinator, batches } = make({ initial: 10 });
		coordinator.beginInitialRender("dev", "P", 3);
		await sleep(50);
		expect(batches).toHaveLength(0);
		await coordinator.flushPending("dev");
		expect(batches).toHaveLength(0);
	});

	test("duplicate frames for the same slot keep the latest one", async () => {
		const { coordinator, batches } = make({ initial: 500 });
		coordinator.beginInitialRender("dev", "P", 2);
		coordinator.queue(frame(0, "old"));
		coordinator.queue(frame(0, "new"));
		coordinator.queue(frame(1));
		await coordinator.flushPending("dev");
		expect(batches[0].find((f) => f.context.position == 0)?.image).toBe("new");
		expect(batches[0]).toHaveLength(2);
	});

	test("keypad and encoder slots with the same position are distinct", async () => {
		const { coordinator, batches } = make({ initial: 500 });
		coordinator.beginInitialRender("dev", "P", 2);
		coordinator.queue(frame(0, "k", "dev", "P", "Keypad"));
		coordinator.queue(frame(0, "e", "dev", "P", "Encoder"));
		await coordinator.flushPending("dev");
		expect(batches[0]).toHaveLength(2);
	});

	test("expectedFrames <= 0 skips collection and goes straight to live mode", async () => {
		const { coordinator, batches } = make({ initial: 1000, live: 5 });
		coordinator.beginInitialRender("dev", "P", -3);
		coordinator.queue(frame(0));
		await sleep(40);
		expect(batches).toHaveLength(1);
	});

	test("after the initial flush further frames use the live window", async () => {
		const { coordinator, batches } = make({ initial: 500, live: 5 });
		coordinator.beginInitialRender("dev", "P", 1);
		coordinator.queue(frame(0));
		await coordinator.flushPending("dev");
		coordinator.queue(frame(1));
		await sleep(40);
		expect(batches).toHaveLength(2);
		expect(batches[1][0].context.position).toBe(1);
	});

	test("beginning a new initial render discards the previous one", async () => {
		const { coordinator, batches } = make({ initial: 500 });
		coordinator.beginInitialRender("dev", "A", 2);
		coordinator.queue(frame(0, "stale", "dev", "A"));
		coordinator.beginInitialRender("dev", "B", 1);
		coordinator.queue(frame(0, "fresh", "dev", "B"));
		await coordinator.flushPending("dev");
		expect(batches).toHaveLength(1);
		expect(batches[0][0].image).toBe("fresh");
	});
});

test.describe("live frames", () => {
	test("coalesces frames queued within the window per slot, latest wins", async () => {
		const { coordinator, batches } = make({ live: 15 });
		coordinator.queue(frame(0, "a"));
		coordinator.queue(frame(0, "b"));
		coordinator.queue(frame(1, "c"));
		await sleep(60);
		expect(batches).toHaveLength(1);
		const byPos = Object.fromEntries(batches[0].map((f) => [f.context.position, f.image]));
		expect(byPos).toEqual({ 0: "b", 1: "c" });
	});

	test("null images are forwarded (clear)", async () => {
		const { coordinator, batches } = make({ live: 5 });
		coordinator.queue(frame(3, null));
		await sleep(40);
		expect(batches[0][0].image).toBeNull();
	});

	test("frames for a different profile than the active render are ignored", async () => {
		const { coordinator, batches } = make({ initial: 500, live: 5 });
		coordinator.beginInitialRender("dev", "P", 1);
		coordinator.queue(frame(0, "x", "dev", "OTHER"));
		await sleep(40);
		await coordinator.flushPending("dev");
		expect(batches).toHaveLength(0);
	});

	test("devices are isolated from each other", async () => {
		const { coordinator, batches } = make({ live: 5 });
		coordinator.queue(frame(0, "a", "d1"));
		coordinator.queue(frame(0, "b", "d2"));
		await sleep(40);
		expect(batches).toHaveLength(2);
	});

	test("frames queued while a send is in flight go out in the next batch", async () => {
		let release!: () => void;
		let first = true;
		const { coordinator, batches } = make({
			live: 2,
			send: () => {
				if (!first) return Promise.resolve();
				first = false;
				return new Promise<void>((resolve) => (release = resolve));
			},
		});
		coordinator.queue(frame(0, "one"));
		await sleep(30);
		expect(batches).toHaveLength(1);
		coordinator.queue(frame(1, "two"));
		await sleep(30);
		expect(batches).toHaveLength(1); // still blocked on the first send
		release();
		await coordinator.flushPending("dev");
		expect(batches).toHaveLength(2);
		expect(batches[1][0].image).toBe("two");
	});

	test("a failing send is reported and later frames are still delivered", async () => {
		let calls = 0;
		const { coordinator, batches, errors } = make({
			live: 2,
			send: async () => {
				if (++calls == 1) throw new Error("boom");
			},
		});
		coordinator.queue(frame(0));
		await sleep(30);
		coordinator.queue(frame(1));
		await sleep(30);
		expect(errors).toHaveLength(1);
		expect((errors[0] as Error).message).toBe("boom");
		expect(batches).toHaveLength(2);
	});
});

test.describe("flushPending", () => {
	test("unknown device resolves immediately", async () => {
		const { coordinator } = make();
		await coordinator.flushPending("nope");
	});

	test("flushes live frames immediately and waits for the send to finish", async () => {
		let finished = false;
		const { coordinator, batches } = make({
			live: 10_000,
			send: async () => {
				await sleep(20);
				finished = true;
			},
		});
		coordinator.queue(frame(0));
		await coordinator.flushPending("dev");
		expect(batches).toHaveLength(1);
		expect(finished).toBe(true);
	});

	test("forces the initial batch out before the timeout", async () => {
		const { coordinator, batches } = make({ initial: 10_000 });
		coordinator.beginInitialRender("dev", "P", 4);
		coordinator.queue(frame(0));
		await coordinator.flushPending("dev");
		expect(batches).toHaveLength(1);
	});
});

test.describe("cancel", () => {
	test("drops pending live frames", async () => {
		const { coordinator, batches } = make({ live: 10 });
		coordinator.queue(frame(0));
		coordinator.cancel("dev");
		await sleep(50);
		expect(batches).toHaveLength(0);
	});

	test("drops collected initial frames and the timeout", async () => {
		const { coordinator, batches } = make({ initial: 15 });
		coordinator.beginInitialRender("dev", "P", 3);
		coordinator.queue(frame(0));
		coordinator.cancel("dev");
		await sleep(50);
		expect(batches).toHaveLength(0);
	});

	test("unknown device is a no-op", () => {
		const { coordinator } = make();
		expect(() => coordinator.cancel("nope")).not.toThrow();
	});

	test("after cancel a new frame starts a fresh live state (profile gate reset)", async () => {
		const { coordinator, batches } = make({ live: 5 });
		coordinator.beginInitialRender("dev", "A", 1);
		coordinator.cancel("dev");
		coordinator.queue(frame(0, "x", "dev", "B"));
		await sleep(40);
		expect(batches).toHaveLength(1);
	});
});

test.describe("defaults", () => {
	test("constructs with only a sender and uses default console reporting", async () => {
		const warnings: unknown[][] = [];
		const original = console.warn;
		console.warn = (...args: unknown[]) => void warnings.push(args);
		try {
			const coordinator = new DeviceFrameCoordinator(async () => {
				throw new Error("x");
			});
			coordinator.queue(frame(0));
			await coordinator.flushPending("dev");
		} finally {
			console.warn = original;
		}
		expect(warnings[0][0]).toBe("Failed to update device images");
	});
});
