QEMU=qemu-system-aarch64
QEMU_CODE=QEMU_CODE.fd
QEMU_VARS=QEMU_VARS.fd

CARGO_FLAGS=

RELEASE ?= 0
ifeq ($(RELEASE), 1)
    CARGO_FLAGS += -r
	ROOT_PATH=root/release
else
	ROOT_PATH=root/debug
endif

QEMU_FLAGS=-machine virt -cpu max \
        -drive if=pflash,format=raw,file=$(QEMU_CODE),readonly=on \
		-drive if=pflash,format=raw,file=$(QEMU_VARS) \
        -drive format=raw,file=fat:rw:`pwd`/$(ROOT_PATH) \
        -net none -monitor stdio -smp 4 -m 256


KERNEL_SRCS=$(shell find ./kernel/src/)
LOADER_SRCS=$(shell find ./loader/src/)

run: build
	$(QEMU) $(QEMU_FLAGS)

debug: build
	$(QEMU) $(QEMU_FLAGS) -s -S

.PHONY: build
build: $(KERNEL_SRCS) kernel/Cargo.toml $(LOADER_SRCS) loader/Cargo.toml
	RUST_TARGET_PATH=`pwd` cargo build $(CARGO_FLAGS)
	cd loader && cargo build $(CARGO_FLAGS) && cd ..

clean:
	cargo clean
	cd loader && cargo clean && cd ..
