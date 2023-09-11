APP_NAME = youtube-audio-feed
IMAGE_HOST = registry.fly.io
IMAGE_NAME = $(IMAGE_HOST)/$(APP_NAME)

TARGET = x86_64-unknown-linux-musl
BINARY = target/$(TARGET)/release/$(APP_NAME)
SOURCES = $(wildcard **/*.rs) Cargo.toml Cargo.lock

VERSION = v$(shell git describe --tags --always --dirty)

$(BINARY): $(SOURCES)
	cargo build --release --target x86_64-unknown-linux-musl

build-docker: $(BINARY)
	(cat Dockerfile.template && \
	echo "\nCOPY $< /$(APP_NAME)\nCMD [\"/$(APP_NAME)\"]\n") | \
		podman build -f - . \
			-t $(IMAGE_NAME):latest \
			-t $(IMAGE_NAME):$(VERSION)

push-docker: build-docker
	podman push $(IMAGE_NAME):latest
	podman push $(IMAGE_NAME):$(VERSION)

deploy: push-docker
	flyctl deploy --image $(IMAGE_NAME):$(VERSION)
.DEFAULT_GOAL := deploy

.PHONY: build-docker push-docker
