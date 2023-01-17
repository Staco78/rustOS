QEMU=qemu-system-aarch64
QEMU_CODE=QEMU_CODE.fd
QEMU_VARS=QEMU_VARS.fd

CARGO_FLAGS=

RELEASE ?= 0
ifeq ($(RELEASE), 1)
    CARGO_FLAGS += -r
	RELEASE_PATH=release
else
	RELEASE_PATH=debug
endif

MODULES_DIRS=$(shell find ./modules/ -maxdepth 1 -type d -not -path "./modules/")
MODULES=$(shell find ./modules/ -maxdepth 1 -type d -not -path "./modules/" | cut -d/ -f3 | awk '$$0="initrd/"$$0".kmod"')
LIBS_DIRS=$(shell find ./libs/ -maxdepth 1 -type d -not -path "./libs/")

QEMU_FLAGS=-machine virt -cpu max \
        -drive if=pflash,format=raw,file=$(QEMU_CODE),readonly=on \
		-drive if=pflash,format=raw,file=$(QEMU_VARS) \
        -drive format=raw,file=fat:rw:`pwd`/root \
        -net none -monitor stdio -smp 4 -m 256 
		# -net none -monitor stdio -smp 4 -m 256 -serial file:log 

export RUST_TARGET_PATH = $(shell pwd)/targets
export RUSTFLAGS = -C symbol-mangling-version=v0 -C metadata=abcd

run: build
	$(QEMU) $(QEMU_FLAGS)

debug: build
	$(QEMU) $(QEMU_FLAGS) -s -S

build: kernel loader initrd.tar

.PHONY: kernel
kernel: 
	cd kernel && RUSTFLAGS="$(RUSTFLAGS) --emit=obj" cargo build $(CARGO_FLAGS) --target aarch64-kernel && cd ..
	# aarch64-linux-gnu-objcopy -R "*exports" -R ".dyn*" target/aarch64-kernel/$(RELEASE_PATH)/kernel build/kernel
	mkdir -p build/kernel_objs
	rm -f build/kernel_objs/*
	cd build/kernel_objs
	ar x target/aarch64-kernel/$(RELEASE_PATH)/libkernel.a --output=build/kernel_objs
	aarch64-linux-gnu-ld build/kernel_objs/* -o build/kernel -Tlinker.ld -x


# .PHONY: initrd/ksymbols
initrd/ksymbols: kernel
	nm -g build/kernel | tools/create_ksymbols.sh

.PHONY: loader
loader:
	cd loader && cargo build $(CARGO_FLAGS) --target aarch64-unknown-uefi && cd ..
	cp target/aarch64-unknown-uefi/$(RELEASE_PATH)/loader.efi build/boot.efi

.PHONY: $(MODULES)
$(MODULES): 
	# cd modules/$(basename $(@F)) && RUSTFLAGS="$(RUSTFLAGS) --emit=obj -Z no-link" cargo build $(CARGO_FLAGS) --target aarch64-modules && cd ../..
	# cp target/aarch64-modules/$(RELEASE_PATH)/deps/$(basename $(@F)).o initrd/$(@F)
	RD=$(RELEASE_PATH) NAME=$(basename $(@F)) ./build_module.sh
	aarch64-linux-gnu-ld build/$(basename $(@F)).o -o build/$(basename $(@F))_.o -r -x -Tmodule-linker.ld
	cargo +stable run --manifest-path=tools/module-postlinker/Cargo.toml -- build/$(basename $(@F))_.o initrd/$(@F)


.PHONY: initrd.tar
initrd.tar: initrd/ksymbols $(MODULES)
	@echo creating initrd...
	$(shell cd initrd && tar -cf ../initrd.tar * -H gnu --no-xattrs && cd ..)

check:
	@cargo check -q --message-format=json --manifest-path=kernel/Cargo.toml --target=targets/aarch64-kernel.json --lib
	@cargo check -q --message-format=json --manifest-path=loader/Cargo.toml --target=aarch64-unknown-uefi
	@cargo +stable check -q --message-format=json --manifest-path=tools/module-postlinker/Cargo.toml
	@(for module in $(MODULES_DIRS) ; do \
        cargo check -q --message-format=json --manifest-path=$$module/Cargo.toml --target=targets/aarch64-modules.json ; \
    done)
	@(for lib in $(LIBS_DIRS) ; do \
        cargo check -q --message-format=json --manifest-path=$$lib/Cargo.toml --target=targets/aarch64-modules.json ; \
    done)

clean:
	cargo clean
	cargo clean --manifest-path=tools/module-postlinker/Cargo.toml
	rm -rf build/*
	rm -rf initrd/*
	rm -f initrd.tar