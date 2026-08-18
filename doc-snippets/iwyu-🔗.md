# dnf

```
dnf install gcc-c++ cmake make python3 libevent-devel boost-devel sqlite-devel zeromq-devel systemtap-sdt-devel capnproto qt6-qtbase-devel qt6-qttools-devel qt6-qtwayland qrencode-devel llvm-devel clang-devel libtool autoconf automake clang llvm lbzip2 patch xz curl htop git vim ccache -y && git clone https://github.com/bitcoin/bitcoin.git  --depth=1 ./b-c && cd b-c
git clone --depth=1 https://github.com/include-what-you-use/include-what-you-use -b clang_20 /include-what-you-use

# apply patch ?


cmake -B /build_iwyu/ -G 'Unix Makefiles' -S /include-what-you-use  -DCMAKE_PREFIX_PATH=/usr/lib/clang/20  # ?
make -C /build_iwyu/ install -j $(nproc)


cmake -B bld-cmake  -DWITH_ZMQ=ON -DBUILD_BENCH=ON -DWITH_ZMQ=ON      -DCMAKE_CXX_COMPILER='clang++;-I/usr/lib/clang/20/include'  -DCMAKE_EXPORT_COMPILE_COMMANDS=ON


python3 "/include-what-you-use/iwyu_tool.py" -p ./bld-cmake -j 9 -- -Xiwyu --cxx17ns -Xiwyu --mapping_file="$PWD/contrib/devtools/iwyu/bitcoin.core.imp"  2>&1 | tee /tmp/iwyu_ci.out


( cd src && python3 "/include-what-you-use/fix_includes.py" --reorder < /tmp/iwyu_ci.out )
