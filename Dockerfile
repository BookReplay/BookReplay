FROM node:25-bookworm-slim AS web-build

WORKDIR /app
COPY app/web/package.json app/web/package-lock.json ./
RUN npm ci
COPY app/web ./
RUN npm run build

FROM rust:1.88-slim-bookworm AS build

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake make \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --locked --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 app
WORKDIR /app
COPY --from=build /app/target/release/bookreplay /usr/local/bin/bookreplay
COPY --from=web-build /app/build ./app/web/build

USER app
# Inside the container the published port decides who can connect.
ENV BIND_ADDR=0.0.0.0:2665
EXPOSE 2665
CMD ["bookreplay"]
