pkgname=kde-gammactl
pkgver=1.1.0
pkgrel=1
pkgdesc="Brightness, contrast and gamma control for KDE Plasma Wayland via ICC profiles"
arch=("x86_64")
url="https://github.com/zNyash/kde-gammactl"
license=("MIT")
depends=("qt6-base" "qt6-declarative")
makedepends=("cargo" "cmake" "ninja" "qt6-declarative")

build() {
    cd "$startdir"
    cargo build --release --locked
    cmake -S gui -B target/gui -G Ninja \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX=/usr
    cmake --build target/gui
}

package() {
    install -Dm755 "$startdir/target/release/kde-gammactl" \
        "$pkgdir/usr/bin/kde-gammactl"
    DESTDIR="$pkgdir" cmake --install "$startdir/target/gui"
    install -Dm644 "$startdir/kde-gammactl.desktop" \
        "$pkgdir/usr/share/applications/kde-gammactl.desktop"
}
