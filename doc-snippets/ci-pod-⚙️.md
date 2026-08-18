# install

```
dnf install -y rsync tmux python3 podman-docker htop git vim && git clone --depth=1 https://github.com/bitcoin/bitcoin ./b-c-ci && cd ./b-c-ci
zypper in -y   rsync tmux python3 podman-docker htop git vim && git clone --depth=1 https://github.com/bitcoin/bitcoin ./b-c-ci && cd ./b-c-ci
export DEBIAN_FRONTEND=noninteractive && apt update && apt install -y rsync tmux python3 podman-docker htop git vim && git clone --depth=1 https://github.com/bitcoin/bitcoin ./b-c-ci && cd ./b-c-ci
```

# single

```
git fetch --depth=1 origin faad066aaf44425eddacf08b0e59fb8d7bb7dddd && git checkout FETCH_HEAD

USER=dummy_user DANGER_RUN_CI_ON_HOST="1" MAKEJOBS="-j$(nproc)" FILE_ENV="./ci/test/00_setup_env_arm.sh" ./ci/test_run_all.sh
DIR_QA_ASSETS=/qa_assets CCACHE_DIR=/ccache_dir CCACHE_MAXSIZE=5500M USER=dummy_user DANGER_RUN_CI_ON_HOST="1" MAKEJOBS="-j$(nproc)" FILE_ENV="./ci/test/00_setup_env_arm.sh" ./ci/test_run_all.sh
```

# CI runner

```
truncate -s 0 /swapfile_ci && chattr +C /swapfile_ci
fallocate -l 22G /swapfile_ci && chmod 600 /swapfile_ci && mkswap /swapfile_ci && swapon /swapfile_ci && ( echo '/swapfile_ci none swap sw 0 0' | tee -a /etc/fstab )

sysctl vm.mmap_rnd_bits=28 # https://github.com/bitcoin/bitcoin/issues/30674 on Ubuntu
sysctl net.ipv6.conf.all.disable_ipv6=0 && podman run --rm --privileged docker.io/multiarch/qemu-user-static --reset -p yes && cd b-c-ci/ && tmux new -s "ci_runner"

podman run --rm -ti --platform linux/s390x "docker.io/debian:bookworm" uname --machine
```

```
# cat run.sh 
set -e
export MAKEJOBS="-j6"  # Try to avoid valgrind timeouts?
# Kill failed containers from previous run?
#podman container rm --all --force  # docker container rm --force $( docker container ls --all --quiet )
git fetch --all
git checkout origin/master -f
git clean -dffx
unset HOST  # For OpenSuse
export TEST_RUNNER_TIMEOUT_FACTOR=480
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_chimera_lto.sh"                              ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_arm.sh"                              ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_i686_no_ipc.sh"                      ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_mac_cross_intel.sh"                  ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_mac_cross.sh"                        ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_asan.sh"                      ./ci/test_run_all.sh ) 
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_alpine_musl.sh"               ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_fuzz.sh"                      ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_fuzz_with_msan.sh"            ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_fuzz_with_valgrind.sh"        ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_msan.sh"                      ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_nowallet_libbitcoinkernel.sh" ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_previous_releases.sh"         ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_tidy.sh"                      ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_tsan.sh"                      ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_native_valgrind.sh"                  ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_s390x.sh"                            ./ci/test_run_all.sh )
docker image prune --force && ( FILE_ENV="./ci/test/00_setup_env_win64.sh"                            ./ci/test_run_all.sh )
```

```
tmux:
podman container rm --all --force && while bash ../run.sh ; do echo 1 >> /tmp/runs ; done

date -r /tmp/runs && wc -l /tmp/runs
df -h ./ && podman image ls && podman container ls && ( date -r /tmp/runs && wc -l /tmp/runs ); dnf update -y && htop
```

# ipv6

```
sed -i 's|bitcoincore.org/depends-sources|drahtbot.space/depends_download_fallback|g' depends/Makefile
```
