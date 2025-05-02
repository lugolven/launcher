
rust-files: **/*.rs Cargo.toml Cargo.lock

target/release/launcher: rust-files
	cargo build --release
	

target/%-unknown-linux-gnu/release/launcher: rust-files
	rustup target add $*-unknown-linux-gnu
	RUSTFLAGS='-C linker=$*-linux-gnu-gcc' cargo build --release --target=$*-unknown-linux-gnu

target/%-unknown-linux-gnu.tar.gz: target/%-unknown-linux-gnu/release/launcher
	tar -czf target/$*-unknown-linux-gnu.tar.gz -C target/$*-unknown-linux-gnu/release launcher

release-archive-%: target/%-unknown-linux-gnu.tar.gz
	@echo "Creating release archive for target/$*-unknown-linux-gnu/$*-unknown-linux-gnu.tar.gz"
	
release-archives: release-archive-x86_64 release-archive-aarch64

remove-draft-releases:
	gh release list --json isDraft,tagName -q 'map(select(.isDraft == true)) | .[] | ( "gh release delete " + .tagName)' | while read -r c; do $$c; done

draft-release: release-archives remove-draft-releases tests
	$(eval VERSION="$(shell date +"%Y-%m-%d-%H%M")")
	gh release create ${VERSION} --title ${VERSION} --draft --generate-notes target/x86_64-unknown-linux-gnu.tar.gz target/aarch64-unknown-linux-gnu.tar.gz

tests:
	cargo test --release

ci: tests draft-release
	@echo "CI build complete"

clean:
	cargo clean
	rm -rf target

