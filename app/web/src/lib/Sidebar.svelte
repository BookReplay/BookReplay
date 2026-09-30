<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/state';

	const items = [
		{ label: 'Today', href: '/' },
		{ label: 'Review', href: '/review/' },
		{ label: 'Books', href: '/?view=books' },
		{ label: 'Progress', href: '/stats/' },
		{ label: 'Settings', href: '/settings/' }
	];
	let streak = $state(0);
	let requestId = 0;

	onMount(() => {
		const loadStreak = async () => {
			const currentRequest = ++requestId;
			try {
				const response = await fetch('/api/reviews/streak');
				if (!response.ok) return;
				const progress = (await response.json()) as { streak: number };
				if (currentRequest === requestId) streak = progress.streak;
			} catch {
				// Leave the badge hidden if progress is unavailable.
			}
		};
		const updateStreak = (event: Event) => {
			requestId += 1;
			streak = (event as CustomEvent<number>).detail;
		};

		void loadStreak();
		window.addEventListener('bookreplay:streak', updateStreak);
		return () => window.removeEventListener('bookreplay:streak', updateStreak);
	});

	function isActive(href: string) {
		if (href === '/?view=books') return browser && page.url.pathname === '/' && page.url.searchParams.get('view') === 'books';
		return page.url.pathname === href && (href !== '/' || !browser || !page.url.searchParams.has('view'));
	}
</script>

<aside class="app-sidebar" aria-label="App navigation">

	<nav aria-label="Primary navigation">
		<ul class="nav-items">
			{#each items as item}
				<li>
					<a
						class:active={isActive(item.href)}
						class="nav-link"
						href={item.href}
						aria-current={isActive(item.href) ? 'page' : undefined}
					>
						<svg class="size-5 shrink-0" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
							{#if item.label === 'Today'}
								<circle cx="8" cy="8" r="3" /><path d="M8 1v1M8 14v1M1 8h1M14 8h1M3 3l1 1M12 12l1 1M3 13l1-1M12 4l1-1" />
							{:else if item.label === 'Review'}
								<rect x="2" y="2" width="12" height="12" rx="2" />
								<path d="M5 5h6M5 8h6M5 11h4" />
							{:else if item.label === 'Books'}
								<path d="M8 3.5C6.5 2.5 4.5 2.5 2 3v10c2.5-.5 4.5-.5 6 .5m0-10c1.5-1 3.5-1 6-.5v10c-2.5-.5-4.5-.5-6 .5m0-10v10" />
							{:else if item.label === 'Highlights'}
								<path d="m9.5 2-6 7 3.5 1.5L5.5 14l7-6-3-1z" />
							{:else if item.label === 'Progress'}
								<path d="M2.5 13.5h11M4 12V8h2v4m1-1V5h2v7m1-2V3h2v9" />
							{:else}
								<path d="M6.5 2.5h3l.4 1.4 1.3.8 1.4-.3 1.5 2.6-1 1 .1 1.5 1 1-1.5 2.6-1.4-.3-1.3.8-.4 1.4h-3l-.4-1.4-1.3-.8-1.4.3L2 10.5l1-1V8l-1-1 1.5-2.6 1.4.3 1.3-.8z" />
								<circle cx="8" cy="8.5" r="2" />
							{/if}
						</svg>
						<span>{item.label}</span>
						{#if item.label === 'Review' && streak > 0}<span class="nav-streak" aria-label={`${streak}-day streak`}>{streak}</span>{/if}
					</a>
				</li>
			{/each}
		</ul>
	</nav>

</aside>

<style>
	.active {
		background: var(--color-sage);
		color: var(--color-forest-dark);
	}
</style>
