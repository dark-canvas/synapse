KERNEL_DEPS_DIR := target/x86_64-unknown-linux-gnu/debug/deps
KERNEL_DEBUG_LINK := $(KERNEL_DEPS_DIR)/kernel-debug

.PHONY: all test doc boot debug clean kernel-debug

all:
	RUSTFLAGS="-C link-arg=-Tmemory_layout.ld" cargo build -Z build-std-features=compiler-builtins-mem -Z build-std=core,compiler_builtins

kernel-debug:
	@mkdir -p $(KERNEL_DEPS_DIR)
	@LATEST=$$(find $(KERNEL_DEPS_DIR) -maxdepth 1 -type f -executable -name 'kernel-*' | sort | tail -n 1); \
	if [ -n "$$LATEST" ]; then \
		ln -sf "$$LATEST" $(KERNEL_DEBUG_LINK); \
	else \
		echo "No runnable kernel test artifact found in $(KERNEL_DEPS_DIR)" >&2; \
		exit 1; \
	fi

test: 
	cargo test --target x86_64-unknown-linux-gnu
	@$(MAKE) --no-print-directory kernel-debug

doc:
	cargo test --target x86_64-unknown-linux-gnu

boot:
	cp target/target.x86_64/debug/kernel ../satus/esp/efi/boot/kernel.elf
	bash -c "../satus/run.sh"

debug:
	cp target/target.x86_64/debug/kernel ../satus/esp/efi/boot/kernel.elf
	bash -c "../satus/debug.sh"

clean:
	cargo clean --target x86_64-unknown-linux-gnu
	cargo clean 
