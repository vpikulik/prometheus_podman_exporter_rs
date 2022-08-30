FROM rust:1.63-alpine3.16 AS build
COPY . /usr/src/exporter/
WORKDIR /usr/src/exporter
RUN apk add musl-dev
RUN cargo build --release
RUN ls -lh target/release

FROM alpine:3.16
COPY --from=build /usr/src/exporter/target/release/prometheus_podman_exporter /app/
WORKDIR /app
USER 1000
LABEL org.opencontainers.image.source https://github.com/vpikulik/prometheus_podman_exporter_rs
