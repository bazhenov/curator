.PHONY: clean curator-frontend.iid

%.iid: src/** Cargo.toml Cargo.toml
	docker build --target=$(basename $@) --iidfile $@ -t $(basename $@):dev .

curator-frontend.iid:
	docker build --target=frontend --iidfile=curator-frontend.iid -t curator-frontend:dev frontend

clean:
	rm -f *.iid

# Enable LLD as a linker. It's 3-5 times faster on Linux but basically broken on macOS:
# https://github.com/rust-lang/rust/issues/39915
.cargo/config:
	echo '[build]' > $@
	echo 'rustflags = ["-C", "link-arg=-fuse-ld=lld"]' >> $@