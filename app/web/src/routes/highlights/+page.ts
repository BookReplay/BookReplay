import { redirect } from '@sveltejs/kit';

export const prerender = true;
export const trailingSlash = 'always';

export function load() {
	redirect(308, '/');
}
