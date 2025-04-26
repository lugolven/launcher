
TARGETS := aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu
rust-files: **/*.rs Cargo.toml Cargo.lock

target/release/launcher: rust-files
	cargo build --release
	
target/x86_64-unknown-linux-gnu/release/launcher: rust-files
	rustup target add x86_64-unknown-linux-gnu
	RUSTFLAGS='-C linker=x86_64-linux-gnu-gcc' cargo build --release --target=x86_64-unknown-linux-gnu

target/aarch64-unknown-linux-gnu/release/launcher: rust-files
	rustup target add aarch64-unknown-linux-gnu
	RUSTFLAGS='-C linker=aarch64-linux-gnu-gcc' cargo build --release --target=aarch64-unknown-linux-gnu


target/x86_64-unknown-linux-gnu/x86_64-unknown-linux-gnu.tar.gz: target/x86_64-unknown-linux-gnu/release/launcher
	tar -czf target/x86_64-unknown-linux-gnu/x86_64-unknown-linux-gnu.tar.gz -C target/x86_64-unknown-linux-gnu/release launcher

target/aarch64-unknown-linux-gnu/aarch64-unknown-linux-gnu.tar.gz: target/aarch64-unknown-linux-gnu/release/launcher
	tar -czf target/aarch64-unknown-linux-gnu/aarch64-unknown-linux-gnu.tar.gz -C target/aarch64-unknown-linux-gnu/release launcher

release-archives: target/x86_64-unknown-linux-gnu/x86_64-unknown-linux-gnu.tar.gz target/aarch64-unknown-linux-gnu/aarch64-unknown-linux-gnu.tar.gz

remove-draft-releases:
	gh release list --json isDraft,tagName -q 'map(select(.isDraft == true)) | .[] | ( "gh release delete " + .tagName)' | while read -r c; do $$c; done

draft-release: release-archives remove-draft-releases tests
	$(eval VERSION="$(shell date +"%Y-%m-%d-%H%M")")
	gh release create ${VERSION} --title ${VERSION} --draft --generate-notes target/x86_64-unknown-linux-gnu/x86_64-unknown-linux-gnu.tar.gz target/aarch64-unknown-linux-gnu/aarch64-unknown-linux-gnu.tar.gz

tests:
	cargo test --release

ci: tests build-release

clean:
	cargo clean
	rm -rf target

