.PHONY = images

# Enable LLD as a linker. It's 3-5 times faster on Linux but basically broken on macOS:
# https://github.com/rust-lang/rust/issues/39915
.cargo/config:
	echo '[build]' > $@
	echo 'rustflags = ["-C", "link-arg=-fuse-ld=lld"]' >> $@

images:
	COMPOSE_DOCKER_CLI_BUILD=1 docker-compose build