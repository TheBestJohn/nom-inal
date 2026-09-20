# ---- build ----------------------------------------------------------------
# On the BUILD platform: the output is static files, identical on every
# architecture, so there is nothing to gain from running the toolchain under
# emulation. Only the nginx layer below varies by platform.
FROM --platform=$BUILDPLATFORM node:24-alpine AS builder

WORKDIR /build

# npm ci against the lockfile alone keeps this layer cached until dependencies
# actually change.
COPY package.json package-lock.json ./
RUN npm ci

COPY tsconfig*.json vite.config.ts index.html sw.js ./
COPY public ./public
COPY src ./src
RUN npm run build

# ---- runtime --------------------------------------------------------------
FROM nginx:1.27-alpine AS runtime

COPY --from=builder /build/dist /usr/share/nginx/html
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD wget -qO- http://127.0.0.1/healthz >/dev/null || exit 1
