export OUT_DIR=`pwd`/build
mkdir -p $OUT_DIR

if [ "$RD" = "release" ]; then
export CORE_ID=05deea1d94c2da97
export ALLOC_ID=78eea7f047a1b607
export BUILTINS_ID=9e353252e2304cbb
export KERNEL_ID=98eef63a49e25a3e
export LOG_ID=1a756713c23803cf
export OPT_LVL=3
else
export CORE_ID=c166a37fcc9bd652
export ALLOC_ID=3dedc08854d5615c
export BUILTINS_ID=cb52a05c20849e61
export KERNEL_ID=3119936a2e234d4d
export LOG_ID=77bef54555a27268
export OPT_LVL=0
fi

export CARGO=`which cargo`
export CARGO_CRATE_NAME=$NAME CARGO_MANIFEST_DIR=`pwd`/modules/$NAME
export CARGO_PKG_AUTHORS='' CARGO_PKG_DESCRIPTION='' CARGO_PKG_HOMEPAGE='' CARGO_PKG_LICENSE=''
export CARGO_PKG_LICENSE_FILE='' CARGO_PKG_NAME=$NAME CARGO_PKG_REPOSITORY=''
export CARGO_PKG_RUST_VERSION='' CARGO_PKG_VERSION=0.1.0 CARGO_PKG_VERSION_MAJOR=0
export CARGO_PKG_VERSION_MINOR=1 CARGO_PKG_VERSION_PATCH=0 CARGO_PKG_VERSION_PRE=''
export CARGO_PRIMARY_PACKAGE=1
export LD_LIBRARY_PATH='`pwd`/target/$RD/deps' 
rustc --crate-name $NAME --edition=2021 modules/$NAME/src/lib.rs --crate-type staticlib \
-C opt-level=$OPT_LVL -C embed-bitcode=no \
--out-dir  $OUT_DIR \
--target aarch64-kernel -C strip=symbols \
-L dependency=`pwd`/target/aarch64-kernel/$RD/deps \
-L dependency=`pwd`/target/$RD/deps \
--extern "noprelude:alloc=`pwd`/target/aarch64-kernel/$RD/deps/liballoc-$ALLOC_ID.rlib" \
--extern "noprelude:compiler_builtins=`pwd`/target/aarch64-kernel/$RD/deps/libcompiler_builtins-$BUILTINS_ID.rlib" \
--extern "noprelude:core=`pwd`/target/aarch64-kernel/$RD/deps/libcore-$CORE_ID.rlib" \
--extern kernel=`pwd`/target/aarch64-kernel/$RD/deps/libkernel-$KERNEL_ID.rlib \
--extern log=`pwd`/target/aarch64-kernel/$RD/deps/liblog-$LOG_ID.rlib \
-Z unstable-options -C symbol-mangling-version=v0 --emit=obj -Z no-link -C code-model=large