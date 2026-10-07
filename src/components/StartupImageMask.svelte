<script lang="ts">
	import { AKP05_MASK } from "$lib/startupImage";
</script>

<svg data-testid="startup-mask" class="pointer-events-none absolute inset-0 h-full w-full" viewBox={`0 0 ${AKP05_MASK.width} ${AKP05_MASK.height}`} preserveAspectRatio="none" aria-hidden="true">
	<defs>
		<mask id="akp05-startup-layout-mask">
			<rect width={AKP05_MASK.width} height={AKP05_MASK.height} fill="white" />
			{#each AKP05_MASK.keyY as y}
				{#each AKP05_MASK.keyX as x}
					<rect {x} {y} width={AKP05_MASK.keySize} height={AKP05_MASK.keySize} rx="11" fill="black" />
				{/each}
			{/each}
			<rect x={AKP05_MASK.touchStrip.x} y={AKP05_MASK.touchStrip.y} width={AKP05_MASK.touchStrip.width} height={AKP05_MASK.touchStrip.height} fill="black" />
		</mask>
	</defs>
	<rect width={AKP05_MASK.width} height={AKP05_MASK.height} fill="rgba(0, 0, 0, 0.82)" mask="url(#akp05-startup-layout-mask)" />
	<g fill="none" stroke="rgba(255, 255, 255, 0.38)" stroke-width="2">
		{#each AKP05_MASK.keyY as y}
			{#each AKP05_MASK.keyX as x}
				<rect {x} {y} width={Math.min(AKP05_MASK.keySize, AKP05_MASK.width - x - 1)} height={AKP05_MASK.keySize} rx="11" />
			{/each}
		{/each}
		<rect x={AKP05_MASK.touchStrip.x} y={AKP05_MASK.touchStrip.y} width={Math.min(AKP05_MASK.touchStrip.width, AKP05_MASK.width - AKP05_MASK.touchStrip.x - 1)} height={AKP05_MASK.touchStrip.height} />
	</g>
</svg>
