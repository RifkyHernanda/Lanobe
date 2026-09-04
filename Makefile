# Lanobe build targets.
#
# The WebUI is embedded into the binary at compile time (rust-embed), so the
# frontend must be built first.

.PHONY: webui server build run clean check

webui:
	cd WebUI && yarn install --frozen-lockfile && yarn build
	rm -rf bin/lanobe/resources/webui
	cp -r WebUI/build bin/lanobe/resources/webui

server:
	cargo build --release

build: webui server

run: build
	./target/release/lanobe

check:
	cargo clippy --all-targets -- -D warnings
	cargo test
	cd WebUI && yarn lint

clean:
	cargo clean
	rm -rf WebUI/build bin/lanobe/resources/webui
