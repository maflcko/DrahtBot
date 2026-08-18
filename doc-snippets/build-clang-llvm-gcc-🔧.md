# clang llvm

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/llvm/llvm-project && cd ./llvm-project && apt install build-essential libtool autotools-dev automake pkg-config bsdmainutils python3 cmake ninja-build python3 llvm clang lld mold -y
cmake -S llvm -B bld -G Ninja -DCMAKE_CXX_COMPILER_LAUNCHER=ccache -DLLVM_USE_LINKER=mold -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER=clang++  -DLLVM_INCLUDE_EXAMPLES=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF -DLLVM_INCLUDE_TESTS=ON  -DCMAKE_BUILD_TYPE=Release -DLLVM_ENABLE_PROJECTS='clang;clang-tools-extra' -DLLVM_ENABLE_RUNTIMES=compiler-rt
ninja -C bld
ninja -C bld -t targets  all
ninja -C bld check-clang-extra-clang-tidy-checkers-performance
ninja -C bld && bld/bin/llvm-lit -sv bld/runtimes/runtimes-bins/compiler-rt/test/msan/X86_64/  --filter sret-origin
```

https://libcxx.llvm.org/BuildingLibcxx.html#the-default-build

# libc++

```
...
cmake -S llvm -B bld -G Ninja -DLLVM_USE_LINKER=mold -DCMAKE_C_COMPILER=clang -DCMAKE_CXX_COMPILER='clang++'  -DLLVM_INCLUDE_EXAMPLES=OFF -DLLVM_INCLUDE_BENCHMARKS=OFF -DLLVM_INCLUDE_TESTS=ON -DLLVM_PARALLEL_LINK_JOBS=9 -DCMAKE_BUILD_TYPE=Release -DLLVM_ENABLE_PROJECTS='' -DLLVM_ENABLE_RUNTIMES='libcxx;libcxxabi' -DLIBCXXABI_USE_LLVM_UNWINDER=OFF && ninja -C bld check-cxx
./bld/bin/llvm-lit -sv ./bld/runtimes/runtimes-bins/libcxx/test/std/time/time.point/time.point.arithmetic
```

# gcc

```
apt update && apt install -y build-essential git libgmp-dev libmpfr-dev libmpc-dev flex     build-essential flex bison texinfo gawk libgmp-dev libmpfr-dev libmpc-dev libisl-dev libzstd-dev dejagnu expect tcl python3-pytest   && git clone https://github.com/gcc-mirror/gcc && cd gcc && mkdir bld && cd bld
../configure --prefix=/opt/gcc-m --enable-languages=c,c++ --disable-multilib --disable-libsanitizer --disable-bootstrap
make -j$(nproc)
make install
```
