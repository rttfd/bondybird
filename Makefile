clippy:
	cargo clippy --all-targets --all-features -- -D warnings -W clippy::pedantic   

test:
	cargo test --all-features -- --test-threads=4

fmt:
	cargo fmt --all -- --check