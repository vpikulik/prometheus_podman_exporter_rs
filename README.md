# Prometheus exporter for Podman metrics

The exporter uses Podman API. The Podman socket should be provided.

Have a look at: https://docs.podman.io/en/latest/markdown/podman-system-service.1.html

## How to build and run

```bash
cargo build --release

./prometheus_podman_exporter -h 0.0.0.0 -p9807 --podman unix://${XDG_RUNTIME_DIR}/podman/podman.sock
```

## Run with podman

```bash
podman run --rm -it \
    -v ${XDG_RUNTIME_DIR}/run/podman:/run/podman:z --security-opt label=disable \
    -p9807:9807 \
    ghcr.io/vpikulik/prometheus_podman_exporter:latest \
    ./prometheus_podman_exporter -h 0.0.0.0 -p9807 --podman unix:///run/podman/podman.sock
```
