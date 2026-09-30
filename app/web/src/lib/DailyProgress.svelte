<script lang="ts">
	let { count, streak }: { count: number; streak: number } = $props();
</script>

<section class="daily-progress" aria-label="Daily reading practice">
	<div class="flex flex-wrap items-center justify-between gap-3">
		<div>
			<p class="eyebrow">A little, every day</p>
			<p class="mt-1 font-serif text-xl">{count >= 5 ? 'Today’s ritual, complete.' : count > 0 ? 'You’re finding your rhythm.' : 'Five ideas. A moment for you.'}</p>
		</div>
		<span class="streak-pill"><span aria-hidden="true">✦</span> {streak > 0 ? `${streak}-day streak` : 'A fresh start'}</span>
	</div>
	<div class="ritual-steps" role="progressbar" aria-label="Daily goal" aria-valuemin={0} aria-valuemax={5} aria-valuenow={Math.min(count, 5)} aria-valuetext={`${count} highlights reviewed today; goal is five`}>
		{#each [1, 2, 3, 4, 5] as step}
			<span class:filled={count >= step} aria-hidden="true">{count >= step ? '✓' : step}</span>
		{/each}
	</div>
	<p class="flex justify-between gap-3 text-xs text-muted"><span>{count} reviewed today</span><span>{count >= 5 ? 'A small habit, well kept.' : `${5 - count} to your daily goal`}</span></p>
</section>
