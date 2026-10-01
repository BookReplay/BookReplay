import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			adapter: adapter({ precompress: true }),
			// Emitted as a <meta> policy in each prerendered page; the server adds the remaining directives.
			csp: { mode: 'hash', directives: { 'script-src': ['self'] } }
		})
	],
	server: {
		proxy: {
			'/api': 'http://localhost:2665'
		}
	}
});
