# The web image: the built client served by Caddy, which also proxies /api, /hooks, /oauth and the health
# checks to the Weft API servers (see the backend's docs/operations.md and deploy/production/compose.yml).
#   docker build -t weft-web .
FROM node:22-bookworm-slim AS build
WORKDIR /src
COPY package.json package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY . .
RUN npm run build

FROM caddy:2.8
COPY deploy/Caddyfile /etc/caddy/Caddyfile
COPY --from=build /src/dist /srv
