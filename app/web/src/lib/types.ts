export type BookSummary = {
	id: number;
	title: string;
	authors: string[];
	cover_url: string | null;
	highlight_count: number;
};

export type BookHighlight = {
	id: number;
	metadata: string;
	content: string;
};

export type BookCandidate = {
	provider: 'open_library' | 'google_books';
	provider_id: string;
	title: string;
	authors: string[];
	cover_url: string | null;
	first_publish_year: number | null;
	edition_count: number | null;
	isbns: string[];
};

export type StreakResponse = {
	streak: number;
	daily_revision_count: number;
	due_count: number;
};
