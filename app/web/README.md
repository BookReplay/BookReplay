# BookReplay web

SvelteKit frontend for importing a Kindle `My Clippings.txt` file into the BookReplay API.

## Developing

Start the API on port 2665, then run:

```sh
npm ci
npm run dev
```

Vite proxies `/api` requests to `http://localhost:2665`, matching the same-origin URL used in production.

## Production

The Docker build prerenders this app into `build/`. The BookReplay Axum server serves those files and the `/api` routes from the same origin:

```sh
docker compose -f compose.yaml -f compose.dev.yaml up --build
```
