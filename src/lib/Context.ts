export type Context = {
	device: string;
	profile: string;
	controller: string;
	position: number;
};

export function contextsEqual(left: Context | null | undefined, right: Context | null | undefined): boolean {
	if (left === right) return true;
	if (!left || !right) return false;
	return left.device == right.device && left.profile == right.profile && left.controller == right.controller && left.position == right.position;
}
