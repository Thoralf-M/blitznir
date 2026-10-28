#!/bin/sh
# Blitznir on Linux (GNOME Wayland first; KDE and X11 best effort):
# system packages, the input group, /dev/uinput access, the tray hint, then
# build and install ~/.local/bin/blitznir. Idempotent: run it again after a
# pull. Asks for sudo for the packages, the group and the udev rule.
#
# The input group lets every process of yours read every keystroke: that is
# how the hotkeys work without a compositor API.
set -eu
cd "$(dirname "$0")/.."

say() { printf '==> %s\n' "$*"; }
relogin=
[ $# -eq 0 ] || { say "usage: $0"; exit 2; }

# 1. packages: build deps and the tray library (dlopened at runtime).
#    Screenshots need nothing: the default [capture] cmd = "" is the
#    desktop's screenshot portal.
if command -v apt-get >/dev/null 2>&1; then
    # libxdo: Freya's tray menus (muda) link it
    pkgs="build-essential curl pkg-config libssl-dev libgtk-3-dev libxdo-dev
          libayatana-appindicator3-dev libasound2-dev libxkbcommon-dev"
    missing=$(for p in $pkgs; do dpkg -s "$p" >/dev/null 2>&1 || printf '%s ' "$p"; done)
    if [ -n "$missing" ]; then
        say "installing: $missing"
        sudo apt-get update
        # shellcheck disable=SC2086
        sudo apt-get install -y $missing
    else
        say "packages: all present"
    fi
elif command -v dnf >/dev/null 2>&1; then
    pkgs="gcc gcc-c++ make curl pkgconf-pkg-config openssl-devel gtk3-devel libxdo-devel
          libappindicator-gtk3-devel alsa-lib-devel libxkbcommon-devel
          gnome-shell-extension-appindicator"
    missing=$(for p in $pkgs; do rpm -q "$p" >/dev/null 2>&1 || printf '%s ' "$p"; done)
    if [ -n "$missing" ]; then
        say "installing: $missing"
        # shellcheck disable=SC2086
        sudo dnf install -y $missing
    else
        say "packages: all present"
    fi
else
    say "neither apt nor dnf: install the dev packages of gtk3, libxdo,"
    say "libayatana-appindicator3, alsa, libxkbcommon and openssl, and run this again"
    exit 1
fi

# 2. hotkeys read /dev/input/event*, which belongs to the input group
getent group input >/dev/null || sudo groupadd --system input
if id -nG "$USER" | tr ' ' '\n' | grep -qx input; then
    say "input group: $USER is a member"
else
    sudo usermod -aG input "$USER"
    say "input group: added $USER"
fi
id -nG | tr ' ' '\n' | grep -qx input || relogin=1

# 3. typing needs rw on /dev/uinput, loaded at every boot
rule='KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"'
rules=/etc/udev/rules.d/99-blitznir-uinput.rules
if [ "$(cat "$rules" 2>/dev/null)" = "$rule" ]; then
    say "udev rule: $rules present"
else
    printf '%s\n' "$rule" | sudo tee "$rules" >/dev/null
    say "udev rule: wrote $rules"
fi
modules=/etc/modules-load.d/uinput.conf
if grep -qx uinput "$modules" 2>/dev/null; then
    say "uinput module: listed in $modules"
else
    printf 'uinput\n' | sudo tee "$modules" >/dev/null
    say "uinput module: wrote $modules"
fi
sudo modprobe uinput
sudo udevadm control --reload
sudo udevadm trigger --sysname-match=uinput
say "/dev/uinput: $(ls -l /dev/uinput 2>&1)"

# 4. vanilla GNOME shows no tray icons without the AppIndicator extension
case "${XDG_CURRENT_DESKTOP:-}" in
*GNOME*)
    if gnome-extensions list --enabled 2>/dev/null | grep -qi appindicator; then
        say "tray: an AppIndicator extension is enabled"
    else
        say "tray: enable an AppIndicator extension or the icon shows nowhere:"
        say "  gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com   (Fedora, upstream)"
        say "  gnome-extensions enable ubuntu-appindicators@ubuntu.com          (Ubuntu)"
    fi
    ;;
esac

# 5. build and install
if ! command -v cargo >/dev/null 2>&1; then
    say "no cargo: curl --proto '=https' -sSf https://sh.rustup.rs | sh, then run this again"
    exit 1
fi
# the .desktop file is named after the windows' app id (crates/app
# APP_ID): that is how GNOME finds the dock icon
bin=blitznir
say "building (release)"
BLITZNIR_COMMIT="$(git rev-parse --short HEAD 2>/dev/null || true)" \
    cargo build --release -p "$bin"
install -Dm755 "target/release/$bin" "$HOME/.local/bin/$bin"
install -Dm644 assets/icon-1024.png "$HOME/.local/share/icons/blitznir.png"
desktop="$HOME/.local/share/applications/$bin.desktop"
mkdir -p "$(dirname "$desktop")"
cat > "$desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Blitznir
Comment=Local dictation, rewrite, translate and chat
Exec=$HOME/.local/bin/$bin
Icon=$HOME/.local/share/icons/blitznir.png
Terminal=false
Categories=Utility;
StartupWMClass=$bin
EOF
say "installed $HOME/.local/bin/$bin and $desktop"

if [ -n "$relogin" ]; then
    say "log out and back in (the input group applies to new sessions), then:"
else
    say "next:"
fi
say "  $bin --check     # every line should say ok / granted / present"
say "  $bin             # or start Blitznir from the app menu"
