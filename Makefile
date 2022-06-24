ifndef DOCKER
	DOCKER = docker
endif

auth:
	@echo ${DOCKER}
	@echo ${CR_PAT}
	echo ${CR_PAT} | ${DOCKER} login ghcr.io -u ${GITHUB_USER} --password-stdin

VERSION = $(shell grep 'version = ' Cargo.toml | head -n1 | sed -r "s/^version = (.+)/\1/")
.PHONY: image
image:
	@echo Build image: ${VERSION}
	${DOCKER} build \
		-t prometheus_podman_exporter:${VERSION} \
		-t ghcr.io/vpikulik/prometheus_podman_exporter:${VERSION} \
		-t prometheus_podman_exporter:latest \
		-t ghcr.io/vpikulik/prometheus_podman_exporter:latest \
		-f Dockerfile .

push_image:
	${DOCKER} push ghcr.io/vpikulik/prometheus_podman_exporter:${VERSION}
	${DOCKER} push ghcr.io/vpikulik/prometheus_podman_exporter:latest
