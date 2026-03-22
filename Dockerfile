FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo doc --no-deps --document-private-items

FROM caddy:2-alpine
COPY --from=builder /app/target/doc /srv/docs
COPY site/index.html /srv/site/index.html
COPY site/Caddyfile /etc/caddy/Caddyfile
EXPOSE 8080
