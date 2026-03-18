
all:
	RUSTFLAGS="-C link-arg=-Tmemory_layout.ld" cargo build -Z build-std-features=compiler-builtins-mem -Z build-std=core,compiler_builtins 

test:
	cargo test --target x86_64-unknown-linux-gnu

boot:
	cp target/target.x86_64/debug/kernel ../satus/esp/efi/boot/kernel.elf
	bash -c "../satus/run.sh"

clean:
	cargo clean --target x86_64-unknown-linux-gnu
	cargo clean 
