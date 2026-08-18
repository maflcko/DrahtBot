# native apt

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && apt install  build-essential cmake pkg-config  python3-zmq libzmq3-dev libevent-dev libboost-dev libsqlite3-dev  systemtap-sdt-dev  libcapnp-dev capnproto  libqrencode-dev qt6-tools-dev qt6-l10n-tools qt6-base-dev  clang llvm libc++-dev libc++abi-dev  mold -y   &&  cmake -B ./bld-cmake -DAPPEND_CXXFLAGS='-O3 -g2' -DAPPEND_CFLAGS='-O3 -g2' -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXE_LINKER_FLAGS=-fuse-ld=mold -DCMAKE_C_COMPILER='clang' -DCMAKE_CXX_COMPILER='clang++' --preset=dev-mode -DCMAKE_EXPORT_COMPILE_COMMANDS=ON                             && cmake --build ./bld-cmake --parallel  $(nproc)
```

# depends native apt

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache mold -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && apt install   build-essential  python3-zmq      cmake  curl g++-multilib  make patch bison g++ pkg-config python3 xz-utils clang lld llvm zip       -y  && ( cd depends && make DEBUG=1 NO_QT=1 NO_WALLET=1 NO_ZMQ=1 NO_UPNP=1 NO_NATPMP=1 -j $(nproc) ) &&  cmake -B ./bld-cmake --toolchain depends/*/toolchain.cmake -DAPPEND_CXXFLAGS='-O3 -g2' -DAPPEND_CFLAGS='-O3 -g2' -DCMAKE_BUILD_TYPE=Debug -DCMAKE_EXE_LINKER_FLAGS=-fuse-ld=mold -DCMAKE_C_COMPILER='clang' -DCMAKE_CXX_COMPILER='clang++' --preset=dev-mode -DCMAKE_EXPORT_COMPILE_COMMANDS=ON                              && cmake --build ./bld-cmake --parallel  $(nproc)
```

# depends native apt libc++

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && apt install  libc++abi-dev libc++-dev clang llvm build-essential libtool autotools-dev automake pkg-config bsdmainutils python3-zmq      make automake cmake curl g++-multilib libtool binutils bsdmainutils pkg-config python3 patch bison        -y  && ( cd depends && make CC=clang CXX="clang++ -stdlib=libc++" DEBUG=1 NO_QT=1 NO_WALLET=1 NO_ZMQ=1 NO_UPNP=1 NO_NATPMP=1 -j $(nproc) ) &&  ./autogen.sh && CONFIG_SITE="$PWD/depends/x86_64-pc-linux-gnu/share/config.site" ./configure  && make -j $(nproc)
```

# native apt nightly clang

```
export LLVM_V=22 && export DEBIAN_FRONTEND=noninteractive && apt-get update && apt-get install -y curl && curl -fLO https://apt.llvm.org/llvm.sh && chmod +x llvm.sh && ./llvm.sh         $LLVM_V                   &&  apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && apt install build-essential cmake pkg-config  python3-zmq libzmq3-dev libevent-dev libboost-dev libsqlite3-dev   libcapnp-dev capnproto  libqrencode-dev qt6-tools-dev qt6-l10n-tools qt6-base-dev   libc++abi-$LLVM_V-dev libc++-$LLVM_V-dev    mold -y   &&  cmake -B ./bld-cmake -DCMAKE_C_COMPILER="clang-$LLVM_V" -DCMAKE_CXX_COMPILER="clang++-$LLVM_V"  -DCMAKE_EXE_LINKER_FLAGS=-fuse-ld=mold --preset=dev-mode      -DWITH_USDT=OFF                       && cmake --build ./bld-cmake --parallel  $(nproc)
```

# depends native dnf
```
dnf install gcc-c++ libtool make autoconf automake python3 clang llvm screen libcxx-devel libcxxabi-devel lbzip2 patch xz cmake     curl wget htop git vim ccache   -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && ( cd depends && make CC=clang CXX=clang++ DEBUG=1 NO_QT=1 NO_WALLET=1 NO_ZMQ=1 NO_UPNP=1 NO_NATPMP=1 -j $(nproc) ) &&  cmake -B ./bld-cmake --toolchain depends/*/toolchain.cmake -DBUILD_FUZZ_BINARY=ON -DBUILD_BENCH=ON                              && cmake --build ./bld-cmake --parallel  $(nproc)
```

# native dnf

```
dnf install gcc-c++ cmake make python3 libevent-devel boost-devel sqlite-devel zeromq-devel systemtap-sdt-devel capnproto capnproto-devel qt6-qtbase-devel qt6-qttools-devel qt6-qtwayland qrencode-devel sqlite-devel zeromq-devel clang llvm libcxx-devel libcxxabi-devel screen htop git vim ccache  -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c &&  rm -rf ./bld-cmake && cmake -B ./bld-cmake -DCMAKE_C_COMPILER='clang' -DCMAKE_CXX_COMPILER='clang++' -DBUILD_GUI=ON -DBUILD_FUZZ_BINARY=ON -DBUILD_BENCH=ON -DWITH_ZMQ=ON -DBUILD_UTIL_CHAINSTATE=ON -DBUILD_KERNEL_LIB=ON                            && cmake --build ./bld-cmake --parallel $(nproc)
```

# depends native zypper

```
zypper in -y which gzip cmake find bison gcc-c++ libtool make autoconf automake python3 clang llvm lbzip2 patch xz      curl wget htop git vim ccache  && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && ( cd depends && make CC=clang CXX=clang++ DEBUG=1 NO_QT=1 NO_WALLET=1 NO_ZMQ=1 NO_UPNP=1 NO_NATPMP=1 -j $(nproc) ) &&  cmake -B ./bld-cmake --toolchain depends/*/toolchain.cmake -DBUILD_FUZZ_BINARY=ON -DBUILD_BENCH=ON                              && cmake --build ./bld-cmake --parallel  $(nproc)
```

# native zypper

```
zypper in -y zeromq-devel libevent-devel boost-devel sqlite3-devel  qt6-base-devel qt6-tools-devel qt6-linguist-devel qrencode-devel      find bison gcc-c++ libtool make autoconf automake python3 clang llvm  libcapnp-devel capnproto patch xz      curl wget htop git vim ccache  && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c &&    rm -rf ./bld-cmake && cmake -B ./bld-cmake -DCMAKE_C_COMPILER='clang' -DCMAKE_CXX_COMPILER='clang++' -DBUILD_GUI=ON -DBUILD_FUZZ_BINARY=ON -DBUILD_BENCH=ON -DWITH_ZMQ=ON -DBUILD_UTIL_CHAINSTATE=ON -DBUILD_KERNEL_LIB=ON                            && cmake --build ./bld-cmake --parallel $(nproc)
```

# depends cross

hosts: https://github.com/bitcoin/bitcoin/tree/master/depends

```
export HOST=arm-linux-gnueabihf && export MAKEJOBS="$(nproc)"
apt update && apt install git vim htop  -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && apt install -y bzip2  make automake cmake curl g++-multilib libtool binutils bsdmainutils pkg-config python3 patch bison  && apt install -y     g++-arm-linux-gnueabihf binutils-arm-linux-gnueabihf   g++-aarch64-linux-gnu binutils-aarch64-linux-gnu    g++-powerpc64-linux-gnu binutils-powerpc64-linux-gnu g++-powerpc64le-linux-gnu binutils-powerpc64le-linux-gnu    g++-riscv64-linux-gnu binutils-riscv64-linux-gnu    g++-s390x-linux-gnu binutils-s390x-linux-gnu    && ( cd depends && make NO_QT=1 "-j${MAKEJOBS}" ) && ./autogen.sh && CONFIG_SITE=$PWD/depends/$HOST/share/config.site ./configure && make "-j${MAKEJOBS}"
```

```
export HOST_ARCH=armhf
dpkg --add-architecture armhf && apt update && apt install libc6:armhf libstdc++6:armhf libfontconfig1:armhf libxcb1:armhf python3-zmq
...
make check
```



# native/libc++/libfuzzer

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 b-c && cd b-c && apt install build-essential libtool autotools-dev automake pkg-config bsdmainutils python3-zmq     libevent-dev libboost-dev  libsqlite3-dev  libdb++-dev clang llvm libc++-dev libc++abi-dev  -y   &&  ./autogen.sh && ./configure CC=clang CXX='clang++ -stdlib=libc++'   --enable-fuzz --with-sanitizers=fuzzer && make -j$(nproc)

mkdir temp_pms

FUZZ=process_messages ./src/test/fuzz/fuzz -workers=9 -jobs=9 ./temp_pms
```

# 32-bit (depends) libfuzzer

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 b-c && cd b-c && apt install build-essential libtool autotools-dev automake pkg-config bsdmainutils python3-zmq make automake cmake curl clang llvm g++-multilib libtool binutils bsdmainutils pkg-config python3 patch bison -y  && ( cd depends && make DEBUG=1 CC='clang -m32' CXX='clang++ -m32' HOST=i686-pc-linux-gnu NO_QT=1 NO_WALLET=1 NO_ZMQ=1 NO_UPNP=1 NO_NATPMP=1 -j $(nproc) ) && ./autogen.sh && CONFIG_SITE="$PWD/depends/i686-pc-linux-gnu/share/config.site" ./configure  --enable-fuzz --with-sanitizers=fuzzer && make  -j $(nproc)
```

# 32-bit libc++ (depends) (focal only?) libfuzzer

```
export V=12 && dpkg --add-architecture i386 && export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && apt install libc++abi-$V-dev:i386 libc++-$V-dev:i386 clang-$V:i386 llvm-$V:i386        make automake cmake curl libtool  bsdmainutils pkg-config patch bison        -y  && ( cd depends && make CC="clang-$V -m32 -O1 -fno-omit-frame-pointer -gline-tables-only -fsanitize=address -fsanitize-address-use-after-scope -fsanitize=fuzzer-no-link" CXX="clang++-$V -m32 -g -O1 -fno-omit-frame-pointer -gline-tables-only -fsanitize=address -fsanitize-address-use-after-scope -fsanitize=fuzzer-no-link -stdlib=libc++" DEBUG=1 NO_QT=1 NO_WALLET=1 NO_ZMQ=1 NO_UPNP=1 NO_NATPMP=1 -j $(nproc) ) && ./autogen.sh && CONFIG_SITE="$PWD/depends/x86_64-pc-linux-gnu/share/config.site" ./configure   --with-sanitizers=fuzzer,address --enable-fuzz --with-seccomp=no --enable-fuzz  && make -j $(nproc)
```

# afl

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && git checkout master && apt install build-essential libtool autotools-dev automake pkg-config bsdmainutils python3-zmq     libevent-dev libboost-dev  clang llvm   -y   &&  ./autogen.sh && apt-get install -y build-essential python3-dev automake git flex bison libglib2.0-dev libpixman-1-dev python3-setuptools  lld llvm llvm-dev clang && apt-get install -y gcc-$(gcc --version|head -n1|sed 's/.* //'|sed 's/\..*//')-plugin-dev libstdc++-$(gcc --version|head -n1|sed 's/.* //'|sed 's/\..*//')-dev && git clone https://github.com/AFLplusplus/AFLplusplus  --depth=1 && make -C AFLplusplus/ source-only && CC=$(pwd)/AFLplusplus/afl-clang-lto CXX=$(pwd)/AFLplusplus/afl-clang-lto++ ./configure --enable-fuzz --with-sanitizers=integer,undefined && make -j $( nproc ) && git clone https://github.com/bitcoin-core/qa-assets --depth=1 && mkdir outdir

FUZZ=process_message AFLplusplus/afl-fuzz -i qa-assets/fuzz_seed_corpus/process_message -o ./outdir -m 500 -t 30000 -- src/test/fuzz/fuzz
AFL_NO_UI=1 AFL_DEBUG=1 FUZZ=process_message AFLplusplus/afl-fuzz -i - -o ./outdir2 -t 10000 -- src/test/fuzz/fuzz
```

# Angora (WIP)

```
podman run -it --rm --privileged debian:bullseye

apt update && apt install curl -y && ( curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh ) && apt install git vim htop -y && git clone https://github.com/AngoraFuzzer/Angora && cd Angora/ && apt install llvm clang cmake zlib1g-dev python-is-python3 -y && source "$HOME/.cargo/env" && ./build/build.sh && cd && export DEBIAN_FRONTEND=noninteractive && apt update && apt install curl wget htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c && apt install build-essential libtool autotools-dev automake pkg-config bsdmainutils python3-zmq     libevent-dev libboost-dev libsqlite3-dev libc++abi-dev libc++-dev    -y   &&  ./autogen.sh && ./configure CC='/Angora/bin/angora-clang' CXX='/Angora/bin/angora-clang++'  --enable-fuzz --disable-asm --with-asm=no && make -j 6 && USE_TRACK=1 make -j 6 && mv src/test/fuzz/fuzz src/test/fuzz/fuzz_track && make clean && ccache --clear && mv src/test/fuzz/fuzz src/test/fuzz/fuzz_fast
```

# hfuzz

```
podman privileged

CC=hfuzz-clang CXX=hfuzz-clang++

honggfuzz --iterations 4 --run_time 2 -s -i ./inputs/ -- ./src/test/fuzz/addition_overflow


mkdir -p inputs/

FUZZ=process_messages honggfuzz/honggfuzz --persistent  -i inputs/ -- src/test/fuzz/fuzz

FUZZ=process_messages honggfuzz/honggfuzz --timeout 999 --threads 9 --persistent  -i ./qa-assets/fuzz_seed_corpus/process_messages -- src/test/fuzz/fuzz
