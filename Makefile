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

run: build
	$(QEMU) $(QEMU_FLAGS)

debug: build
	$(QEMU) $(QEMU_FLAGS) -s -S

build: kernel loader initrd.tar

.PHONY: kernel
kernel: 
	cd kernel && cargo build $(CARGO_FLAGS) --target aarch64-kernel && cd ..
	aarch64-linux-gnu-objcopy -R "*exports" -R ".dyn*" target/aarch64-kernel/$(RELEASE_PATH)/kernel build/kernel

.PHONY: initrd/ksymbols
initrd/ksymbols: kernel build/symbols
	nm -Dg target/aarch64-kernel/$(RELEASE_PATH)/kernel | grep -f build/symbols | scripts/create_ksymbols.sh

build/symbols: kernel
	aarch64-linux-gnu-objcopy --dump-section .sym_exports=build/symbols target/aarch64-kernel/$(RELEASE_PATH)/kernel

build/defs: kernel
	aarch64-linux-gnu-objcopy --dump-section .defs_exports=build/defs target/aarch64-kernel/$(RELEASE_PATH)/kernel

.PHONY: loader
loader:
	cd loader && cargo build $(CARGO_FLAGS) --target aarch64-unknown-uefi && cd ..
	cp target/aarch64-unknown-uefi/$(RELEASE_PATH)/loader.efi build/boot.efi

.PHONY: $(MODULES)
$(MODULES): build/defs
	cd modules/$(basename $(@F)) && cargo build $(CARGO_FLAGS) --target aarch64-modules && cd ../..
	cp target/aarch64-modules/$(RELEASE_PATH)/lib$(basename $(@F)).so initrd/$(@F)

.PHONY: initrd.tar
initrd.tar: initrd/ksymbols $(MODULES)
	@echo creating initrd...
	$(shell cd initrd && tar -cf ../initrd.tar * -H gnu --no-xattrs && cd ..)

check:
	@cargo check -q --message-format=json --manifest-path=kernel/Cargo.toml --target=targets/aarch64-kernel.json
	@cargo check -q --message-format=json --manifest-path=loader/Cargo.toml --target=aarch64-unknown-uefi
	@(for module in $(MODULES_DIRS) ; do \
        cargo check -q --message-format=json --manifest-path=$$module/Cargo.toml --target=targets/aarch64-modules.json ; \
    done)
	@(for lib in $(LIBS_DIRS) ; do \
        cargo check -q --message-format=json --manifest-path=$$lib/Cargo.toml --target=targets/aarch64-modules.json ; \
    done)

clean:
	cargo clean
	rm symbols
	rm initrd.tar