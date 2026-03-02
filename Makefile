
all:
	cargo build -Z build-std-features=compiler-builtins-mem -Z build-std=core,compiler_builtins

test:
	cargo test --target x86_64-unknown-linux-gnu

clean:
	cargo clean --target x86_64-unknown-linux-gnu
	cargo clean 
