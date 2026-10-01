import type { StreakResponse } from './types';

// One copy of the owner's daily progress, shared by the sidebar and the pages.
export const progress = $state({ loaded: false, streak: 0, dailyRevisionCount: 0, dueCount: 0 });

let pending: Promise<boolean> | undefined;
let revision = 0;

/** Refreshes progress from the server; concurrent callers share one request. */
export function loadProgress(): Promise<boolean> {
	const started = revision;
	pending ??= fetch('/api/reviews/streak')
		.then(async (response) => {
			if (!response.ok) return false;
			const body = (await response.json()) as StreakResponse;
			// A review saved while this request was in flight is newer than its answer.
			if (started === revision) {
				progress.streak = body.streak;
				progress.dailyRevisionCount = body.daily_revision_count;
				progress.dueCount = body.due_count;
				progress.loaded = true;
			}
			return true;
		})
		.catch(() => false)
		.finally(() => (pending = undefined));
	return pending;
}

export function recordReview(streak: number, dailyRevisionCount: number) {
	revision += 1;
	progress.streak = streak;
	progress.dailyRevisionCount = dailyRevisionCount;
	progress.dueCount = Math.max(0, progress.dueCount - 1);
	progress.loaded = true;
}
