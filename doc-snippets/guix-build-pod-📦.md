```
docker run --privileged -it --rm ubuntu:24.04

fallocate -l 12G /swapfile_ci && chmod 600 /swapfile_ci && mkswap /swapfile_ci && swapon /swapfile_ci && ( echo '/swapfile_ci none swap sw 0 0' | tee -a /etc/fstab )
```

1.4 git
========

```
export DEBIAN_FRONTEND=noninteractive && cd && sed -i 's|# deb-src|deb-src|g' /etc/apt/sources.list && apt update && apt-get build-dep -y guix && apt install vim netbase ca-certificates git -y && git clone --depth=1 --branch=v1.4.0 https://git.savannah.gnu.org/git/guix.git && cd guix && groupadd --system guixbuild && for i in `seq -w 1 10`; do useradd -g guixbuild -G guixbuild -d /var/empty -s `which nologin` -c "Guix build user $i" --system guixbuilder$i; done
```

```diff
diff --git a/gnu/packages/tls.scm b/gnu/packages/tls.scm
index f1e844b..1077c4b 100644
--- a/gnu/packages/tls.scm
+++ b/gnu/packages/tls.scm
@@ -493,6 +493,7 @@ (define-public openssl-1.1
     (native-inputs (list perl))
     (arguments
      `(#:parallel-tests? #f
+       #:tests? #f
        #:test-target "test"
 
        ;; Changes to OpenSSL sometimes cause Perl to "sneak in" to the closure,
```

```
./bootstrap && ./configure --localstatedir=/var && make clean && make -j $( nproc ) && make install && ( LC_ALL=en_US.UTF-8 guix-daemon --build-users-group=guixbuild & )
EDITOR=vim  guix edit openssl

guix install bash



...
```

manual setup
===========================

```
export DEBIAN_FRONTEND=noninteractive && cd && apt update && apt install -y ca-certificates netbase xz-utils git vim htop make bash curl && curl -fLO https://ftpmirror.gnu.org/gnu/guix/guix-binary-1.5.0.armhf-linux.tar.xz  && tar -xf ./guix-binary-*-linux.tar.xz && mv var/guix /var/ && mv gnu / && mkdir -p /config_guix/ && ln -sf /var/guix/profiles/per-user/root/current-guix /config_guix/current && source /config_guix/current/etc/profile && groupadd --system guixbuild && for i in `seq -w 1 10`; do useradd -g guixbuild -G guixbuild -d /var/empty -s `which nologin` -c "Guix build user $i" --system guixbuilder$i; done

LC_ALL=en_US.UTF-8 guix-daemon --build-users-group=guixbuild &


source /config_guix/current/etc/profile 
guix archive --authorize < /config_guix/current/share/guix/ci.guix.info.pub 
guix install hello


```


debian:trixie or ubuntu:noble
============================

```
export DEBIAN_FRONTEND=noninteractive && apt update && apt install git vim htop guix bash curl make -y && groupadd --system guixbuild && for i in `seq -w 1 10`; do useradd -g guixbuild -G guixbuild -d /var/empty -s `which nologin` -c "Guix build user $i" --system guixbuilder$i; done

LC_ALL=en_US.UTF-8 guix-daemon --build-users-group=guixbuild &

source /config_guix/current/etc/profile 
guix archive --authorize < /config_guix/current/share/guix/ci.guix.info.pub 
guix install hello

```

# run

```
git clone https://github.com/bitcoin/bitcoin.git b-c
FORCE_DIRTY_WORKTREE=1 HOSTS=arm-linux-gnueabihf V=1 VERBOSE=1 JOBS=1 ./contrib/guix/guix-build

curl -fLO 'https://bitcoincore.org/depends-sources/sdks/Xcode-26.1.1-17B100-extracted-SDK-with-libcxx-headers.tar'

mkdir -p ./depends/SDKs/ && tar -xvf ./Xcode-*-extracted-SDK-with-libcxx-headers.tar --directory ./depends/SDKs/

uname --machine && find guix-build-$(git rev-parse --short=12 HEAD)/output/ -type f -print0 | env LC_ALL=C sort -z | xargs -r0 sha256sum
```

```
# fix issues:

EDITOR=vim guix edit python-h2  # for slow aarch64


EDITOR=vi guix edit openssl
( edit /usr/share/guile/site/3.0/gnu/packages/tls.scm with above patch)


guix time-machine --commit=bbec79fd55ba8efe4cb015bd07e4f40fb7d252d1 -- install --cores=$( nproc )  --dry-run   rust

guix pull --url=https://codeberg.org/guix/guix.git --commit=53396a22afc04536ddf75d8f82ad2eafa5082725 --cores=1 --max-jobs=1
guix time-machine --url=https://codeberg.org/guix/guix.git --commit=f3e74ef25d479ed77f7cc029d312425b7776c3b0 -- build --cores=1 --max-jobs=1 clisp

guix gc -D  /gnu/store/dfffpg2fw87j67q1a30r2fbjajdrq870-guile-3.0.7.tar.xz
  282  guix download https://www.gnupg.org/ftp/gcrypt/gnutls/v3.7/gnutls-3.7.7.tar.xz
  284  guix download https://gnupg.org/ftp/gcrypt/libgcrypt/libgcrypt-1.10.1.tar.bz2 
  286  guix download https://gnupg.org/ftp/gcrypt/libgpg-error/libgpg-error-1.45.tar.bz2
  360  guix download https://www.gnupg.org/ftp/gcrypt/gnutls/v3.7/gnutls-3.7.2.tar.xz
  361  guix download https://gnupg.org/ftp/gcrypt/libgcrypt/libgcrypt-1.8.8.tar.bz2
  362  guix download https://gnupg.org/ftp/gcrypt/libgpg-error/libgpg-error-1.42.tar.bz2
  384  guix download https://mirror.koddos.net/gcc/releases/gcc-7.5.0/gcc-7.5.0.tar.xz 
  386  guix download https://mirror.koddos.net/gcc/releases/gcc-7.5.0/gcc-7.5.0.tar.xz 
  446  guix download https://www.gnupg.org/ftp/gcrypt/gnutls/v3.7/gnutls-3.8.3.tar.xz
  447  guix download https://www.gnupg.org/ftp/gcrypt/gnutls/v3.8/gnutls-3.8.3.tar.xz

