pkgname=kde-gammactl
pkgver=1.1.0
pkgrel=1
pkgdesc="Brightness, contrast and gamma control for KDE Plasma Wayland via ICC profiles"
arch=("x86_64")
url="https://github.com/zNyash/kde-gammactl"
license=("MIT")
makedepends=("cargo")

build() {
    cd "$startdir"
    cargo build --release --locked
}

package() {
    install -Dm755 "$startdir/target/release/kde-gammactl" \
        "$pkgdir/usr/bin/kde-gammactl"
}
