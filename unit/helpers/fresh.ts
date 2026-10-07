let counter = 0;

// Import a src/lib module as a brand-new instance so module-level state
// (stores, caches, counters) never leaks between tests.
export async function freshImport<T = any>(name: string): Promise<T> {
	return (await import(`../../src/lib/${name}?fresh=${++counter}`)) as T;
}

export const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

// Let queued microtasks (awaited invoke chains) settle.
export async function flushMicrotasks(rounds = 10) {
	for (let i = 0; i < rounds; i++) await Promise.resolve();
	await sleep(0);
}
