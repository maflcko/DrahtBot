```
run-clang-tidy -config-file=./src/.clang-tidy -p=./bld-cmake/ -fix -j $(nproc) ; echo $?
./contrib/devtools/run-clang-tidy.py -config-file=./src/.clang-tidy -p=./bld-cmake/ -fix -j $(nproc) ; echo $?

git diff src/bitcoin-cli.cpp | clang-tidy-diff.py -config-file=./src/.clang-tidy -p1 -path ./bld-cmake/  ; echo $?
git diff | ( cd ./src/ && clang-tidy-diff -p2 -path ../bld-cmake -j $(nproc) ) ; echo $?
git diff | ( cd ./src/ && ../contrib/devtools/clang-tidy-diff.py -p2 -path ../bld-cmake -j $(nproc) ) ; echo $?

clang-tidy -p=./bld-cmake/ src/test/result_tests.cpp
```

https://github.com/bitcoin/bitcoin/blob/master/doc/developer-notes.md#running-clang-tidy
