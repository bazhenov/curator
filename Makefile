.PHONY = images run run-tests

# Enable LLD as a linker. It's 3-5 times faster on Linux but basically broken on macOS:
# https://github.com/rust-lang/rust/issues/39915
.cargo/config:
	echo '[build]' > $@
	echo 'rustflags = ["-C", "link-arg=-fuse-ld=lld"]' >> $@

images:
	COMPOSE_DOCKER_CLI_BUILD=1 docker-compose build

run:
	docker-compose up backend agent frontend router

run-tests:
	docker-compose run it-tests
	docker-compose down -t 0

toolchains/%:
	$(eval IMAGE_ID = bazhenov.me/curator/toolchain-$(@F):dev)
	docker build -t "$(IMAGE_ID)" "$@"
	@echo Toolchain container available as $(IMAGE_ID)