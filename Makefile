src := Cargo.toml Cargo.lock $(wildcard .cargo/*) $(wildcard src/*)

all: x86_64-unknown-linux-musl.zip x86_64-pc-windows-gnu.zip

x86_64-unknown-linux-musl.zip: $(src)
	cargo build --release --target x86_64-unknown-linux-musl
	cd target/x86_64-unknown-linux-musl/release/ && zip x86_64-unknown-linux-musl.zip pexshell && mv x86_64-unknown-linux-musl.zip ../../../

x86_64-pc-windows-gnu.zip: $(src)
	cargo build --release --target x86_64-pc-windows-gnu
	cd target/x86_64-pc-windows-gnu/release/ && zip x86_64-pc-windows-gnu.zip pexshell.exe && mv x86_64-pc-windows-gnu.zip ../../../

x86_64-apple-darwin.zip: $(src)
	cargo build --release --target x86_64-apple-darwin
	cd target/x86_64-apple-darwin/release/ && zip x86_64-apple-darwin.zip pexshell && mv x86_64-apple-darwin.zip ../../../

aarch64-apple-darwin.zip: $(src)
	cargo build --release --target aarch64-apple-darwin
	cd target/aarch64-apple-darwin/release/ && zip aarch64-apple-darwin.zip pexshell && mv aarch64-apple-darwin.zip ../../../

clean:
	rm -f x86_64-unknown-linux-musl.zip x86_64-pc-windows-gnu.zip x86_64-apple-darwin.zip aarch64-apple-darwin.zip
	cargo clean

.PHONY: clean
