# syntax=docker/dockerfile:1.7

FROM rust:alpine AS builder
WORKDIR /app

RUN apk add --no-cache build-base

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src
COPY fonts ./fonts

# Build release binary (use incremental layer caching if possible)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
  --mount=type=cache,target=/app/target \
  cargo build --release && \
  install -Dm755 target/release/excaliosa /out/excaliosa


FROM alpine:3.20 AS runtime

# Create an unprivileged user (Alpine uses adduser/addgroup)
RUN addgroup -S app && adduser -S -G app -u 10001 appuser

# Copy the compiled binary
COPY --from=builder /out/excaliosa /usr/local/bin/excaliosa
COPY LICENSE fonts/*-LICENSE.txt /usr/share/licenses/excaliosa/

USER appuser
ENTRYPOINT ["/usr/local/bin/excaliosa"]
