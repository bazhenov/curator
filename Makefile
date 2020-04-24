.PHONY: clean curator-frontend.iid agent

PROJECT_FILES := src/** Cargo.toml Cargo.toml

%.iid: $(PROJECT_FILES)
	docker build --target=$(basename $@) --iidfile $@ -t $(basename $@):dev .

curator-frontend.iid:
	docker build --target=frontend --iidfile=$@ -t $(basename $@):dev frontend

clean:
	rm -f *.iid

# Enable LLD as a linker. It's 3-5 times faster on Linux but basically broken on macOS:
# https://github.com/rust-lang/rust/issues/39915
.cargo/config:
	echo '[build]' > $@
	echo 'rustflags = ["-C", "link-arg=-fuse-ld=lld"]' >> $@