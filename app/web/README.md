# Rekindle web

SvelteKit frontend for importing a Kindle `My Clippings.txt` file into the Rekindle API.

## Developing

Start the API on port 3000, then run:

```sh
npm ci
npm run dev
```

Vite proxies `/api` requests to `http://localhost:3000`, matching the same-origin URL used in production.

## Production

The Docker build prerenders this app into `build/`. The Rekindle Axum server serves those files and the `/api` routes from the same origin:

```sh
docker compose up --build
```
