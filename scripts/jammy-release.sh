#!/usr/bin/env bash
# Build in the oldest supported userspace, or launch the packaged binary in a runtime-only root.
set -euo pipefail
cache="$PWD/target/release-jammy"
base=ubuntu-base-22.04.5-base-amd64.tar.gz
mode=${1:?build or smoke}
case "$mode" in build|smoke) ;; *) exit 2 ;; esac
mkdir -p "$cache"
if [[ ! -f "$cache/$base" ]]; then
    curl --fail --location --retry 3 "https://cdimage.ubuntu.com/ubuntu-base/releases/22.04/release/$base" -o "$cache/$base.part"
    mv "$cache/$base.part" "$cache/$base"
fi
echo "242cd8898b33ea806ef5f13b1076ed7c76f9f989d18384452f7166692438ff1a  $cache/$base" | sha256sum --check
work=$(mktemp -d "$cache/$mode.XXXXXXXX")
trap 'chmod -R u+rwX "$work" && rm -rf "$work"' EXIT
root="$work/root"
mkdir "$root"
tar --no-same-owner -xzf "$cache/$base" -C "$root"
cp -L --remove-destination /etc/resolv.conf "$root/etc/resolv.conf"
# The rootless runtime has one mapped group. Pre-create the optional shared
# font directory so fontconfig does not try to assign it to the staff group.
mkdir -p "$root/etc/ssl/certs" "$root/usr/local/share/fonts"
cp /etc/ssl/certs/ca-certificates.crt "$root/etc/ssl/certs/"
sed -i -e 's|http://|https://|g' -e '/^deb .* restricted$/s/ restricted$//' -e '/^deb .* multiverse$/d' "$root/etc/apt/sources.list"
box=(bwrap --unshare-user --uid 0 --gid 0 --bind "$root" / --dev /dev --proc /proc --tmpfs /tmp --clearenv --setenv PATH /usr/bin:/bin:/usr/sbin:/sbin --setenv HOME /tmp --setenv DEBIAN_FRONTEND noninteractive)
if [[ "$mode" == build ]]; then
    deps=(build-essential git pkg-config libxkbcommon-dev libwayland-dev)
else
    deps=(xvfb xauth x11-utils libxcursor1 libxkbcommon0 libxkbcommon-x11-0 libegl1 libgl1-mesa-dri libglx-mesa0)
fi
"${box[@]}" apt-get -o Acquire::Languages=none -o APT::Sandbox::User=root update -qq
"${box[@]}" apt-get -o Acquire::Languages=none -o APT::Sandbox::User=root install -y --no-install-recommends "${deps[@]}"
if [[ "$mode" == build ]]; then
    target=x86_64-unknown-linux-gnu
    cargo fetch --locked --target "$target"
    mkdir -p "$work/src/.git" "$cache/target"
    git archive HEAD | tar -xf - -C "$work/src"
    "${box[@]}" --ro-bind "$work/src" /src --ro-bind "$(git rev-parse --absolute-git-dir)" /src/.git \
        --ro-bind "$(rustc --print sysroot)" /opt/rust --bind "$cache/target" /target \
        --bind "${CARGO_HOME:-$HOME/.cargo}/registry" /cargo/registry --setenv CARGO_HOME /cargo \
        --setenv PATH /opt/rust/bin:/usr/bin:/bin --setenv CARGO_TARGET_DIR /target \
        --setenv RUSTFLAGS '--remap-path-prefix=/src=/corktop --remap-path-prefix=/cargo=/rust-dependencies' \
        --chdir /src cargo build --offline --release --locked --target "$target" -j "${CARGO_BUILD_JOBS:-2}" -p corktop
else
    mkdir "$work/app"
    tar -xzf "$2" --strip-components=1 -C "$work/app"
    "${box[@]}" --ro-bind "$work/app" /app --setenv LIBGL_ALWAYS_SOFTWARE 1 --chdir /tmp xvfb-run -a sh -ec '
        /app/corktop --identity
        /app/corktop --health
        /app/corktop --ready
        /app/corktop --probe /tmp/board
        /app/corktop --probe /tmp/board
        /app/corktop --version-window &
        pid=$!
        for i in $(seq 1 100); do
            if xwininfo -root -tree | grep -iq corktop; then kill "$pid"; exit 0; fi
            kill -0 "$pid"
            sleep .1
        done
        exit 1'
fi
