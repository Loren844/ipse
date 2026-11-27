# syntax=docker/dockerfile:1
#
# Ipse on a home server (decision 0019; docs/guide/self-hosting.md): `ipse serve`,
# the static binary alone on a distroless base, as a user who is not root.
#
#   docker build -t ipse .
#
# The release workflow (.github/workflows/docker.yml) builds it for linux/amd64 and
# linux/arm64 and publishes it as ghcr.io/loren844/ipse.

FROM rust:1.97.0-alpine AS build
RUN apk add --no-cache musl-dev
# The image's own Rust, without the components rust-toolchain.toml adds for development.
ENV RUSTUP_TOOLCHAIN=1.97.0
WORKDIR /src
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p ipse \
    && cp target/release/ipse /ipse \
    && mkdir -p /folders/vault /folders/secrets

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /ipse /usr/local/bin/ipse
COPY --from=build --chown=65532:65532 /folders/ /
# No keychain in a container: the sources' secrets go to a file of their own, in a
# volume outside the memory.
ENV IPSE_SECRETS_FILE=/secrets/secrets.json
# The memory is the current folder: `ipse init`, `ipse access-code`… need no --vault.
WORKDIR /vault
VOLUME ["/vault", "/secrets"]
EXPOSE 7420
# Up when the server answers on its port; no shell nor curl in this image: the binary
# asks itself. Another port given to `serve` needs the same one here.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["ipse", "health", "7420"]
ENTRYPOINT ["ipse"]
# Reached from the network: say so explicitly (decision 0019).
CMD ["serve", "--listen", "0.0.0.0:7420"]
