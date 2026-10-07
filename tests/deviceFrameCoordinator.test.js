// @ts-nocheck -- Bun provides these Node test modules at runtime.
import assert from "node:assert/strict";
import { describe, test } from "node:test";

import { DeviceFrameCoordinator } from "../src/lib/DeviceFrameCoordinator.ts";

const context = (profile, position) => ({
	device: "device-a",
	profile,
	controller: "Keypad",
	position,
});

describe("DeviceFrameCoordinator", () => {
	test("waits for every initial slot and sends one batch", async () => {
		const batches = [];
		const coordinator = new DeviceFrameCoordinator(async (frames) => batches.push(frames), 10_000, 10_000);
		coordinator.beginInitialRender("device-a", "Profile A", 3);

		coordinator.queue({ context: context("Profile A", 0), image: "first" });
		coordinator.queue({ context: context("Profile A", 1), image: null });
		assert.equal(batches.length, 0);

		coordinator.queue({ context: context("Profile A", 2), image: "third" });
		await coordinator.flushPending("device-a");

		assert.equal(batches.length, 1);
		assert.deepEqual(
			batches[0].map((frame) => [frame.context.position, frame.image]),
			[
				[0, "first"],
				[1, null],
				[2, "third"],
			],
		);
	});

	test("ignores late frames from the previous profile", async () => {
		const batches = [];
		const coordinator = new DeviceFrameCoordinator(async (frames) => batches.push(frames), 10_000, 10_000);
		coordinator.beginInitialRender("device-a", "Profile B", 1);

		coordinator.queue({ context: context("Profile A", 0), image: "stale" });
		coordinator.queue({ context: context("Profile B", 0), image: "current" });
		await coordinator.flushPending("device-a");

		assert.equal(batches.length, 1);
		assert.equal(batches[0][0].image, "current");
	});

	test("coalesces live frames and keeps the newest frame per slot", async () => {
		const batches = [];
		const coordinator = new DeviceFrameCoordinator(async (frames) => batches.push(frames), 10_000, 10_000);
		coordinator.beginInitialRender("device-a", "Profile A", 1);
		coordinator.queue({ context: context("Profile A", 0), image: "initial" });
		await coordinator.flushPending("device-a");

		coordinator.queue({ context: context("Profile A", 0), image: "older" });
		coordinator.queue({ context: context("Profile A", 0), image: "newer" });
		await coordinator.flushPending("device-a");

		assert.equal(batches.length, 2);
		assert.equal(batches[1].length, 1);
		assert.equal(batches[1][0].image, "newer");
	});

	test("keeps only the latest pending frame while a HID batch is slow", async () => {
		const batches = [];
		let releaseFirstBatch;
		const firstBatchBlocked = new Promise((resolve) => (releaseFirstBatch = resolve));
		const coordinator = new DeviceFrameCoordinator(
			async (frames) => {
				batches.push(frames);
				if (batches.length === 1) await firstBatchBlocked;
			},
			10_000,
			10_000,
		);
		coordinator.beginInitialRender("device-a", "Profile A", 1);
		coordinator.queue({ context: context("Profile A", 0), image: "initial" });

		let finished;
		for (let index = 0; index < 10_000; index++) {
			coordinator.queue({ context: context("Profile A", 0), image: `frame-${index}` });
			finished = coordinator.flushPending("device-a");
		}
		releaseFirstBatch();
		await finished;

		assert.equal(batches.length, 2);
		assert.equal(batches[1].length, 1);
		assert.equal(batches[1][0].image, "frame-9999");
	});
});
