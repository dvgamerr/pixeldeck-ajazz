// Svelte actions that move an element out of its component tree.
function portalTo(target: HTMLElement | null | undefined) {
	return (node: HTMLElement) => {
		target?.appendChild(node);

		return {
			destroy() {
				node.remove();
			},
		};
	};
}

export function portalToBody(node: HTMLElement) {
	return portalTo(document.body)(node);
}

export function portalToPreviewDock(node: HTMLElement) {
	return portalTo(document.querySelector<HTMLElement>(".device-workspace"))(node);
}
