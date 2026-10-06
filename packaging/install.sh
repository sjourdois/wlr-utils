#!/bin/sh
# Install wlr-utils: the six binaries, the systemd user units, the themes, the README, the
# example configuration and the licences. It runs from a source tree, after
# `cargo build --release -p wlr-utils`, or from the unpacked release archive, where
# everything sits next to it.
#
#   sudo sh packaging/install.sh                           into /usr/local, from a source tree
#   sudo sh install.sh                                     the same, from the release archive
#   PREFIX=~/.local sh install.sh                          for yourself alone, without root
#   DESTDIR="$pkgdir" PREFIX=/usr sh packaging/install.sh  for a package
#
# Each directory can be set on its own: BINDIR, DATADIR, SYSTEMDUSERUNITDIR, DOCDIR,
# LICENSEDIR. BINSRC is where the built binaries are. Under $HOME, the units go in
# $DATADIR/systemd/user, where systemd --user looks for them. A missing file stops the
# script before it installs anything.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
if [ -f "$here/../crates/wlr-utils/Cargo.toml" ]; then
	src=$(dirname "$here")
	: "${BINSRC:=$src/target/release}"
	overlayd_unit=$src/crates/wlr-chooser/contrib/wlr-overlayd.service
	draw_unit=$src/crates/wlr-draw/contrib/wlr-draw.service
	themes=$src/docs/themes
	example=$src/docs/config.toml
else
	src=$here
	: "${BINSRC:=$src}"
	overlayd_unit=$src/wlr-overlayd.service
	draw_unit=$src/wlr-draw.service
	themes=$src/themes
	example=$src/config.toml
fi

DESTDIR=${DESTDIR:-}
PREFIX=${PREFIX:-/usr/local}
BINDIR=${BINDIR:-$PREFIX/bin}
DATADIR=${DATADIR:-$PREFIX/share}
user_units=$PREFIX/lib/systemd/user
if [ -n "${HOME:-}" ]; then
	case $PREFIX in
	"$HOME" | "$HOME"/*) user_units=$DATADIR/systemd/user ;;
	esac
fi
SYSTEMDUSERUNITDIR=${SYSTEMDUSERUNITDIR:-$user_units}
DOCDIR=${DOCDIR:-$DATADIR/doc/wlr-utils}
LICENSEDIR=${LICENSEDIR:-$DATADIR/licenses/wlr-utils}

BINS="wlr-chooser wlr-switcher wlr-overlayd wlr-peek wlr-shot wlr-draw"

missing=
for bin in $BINS; do
	[ -f "$BINSRC/$bin" ] || missing="$missing $BINSRC/$bin"
done
for file in "$overlayd_unit" "$draw_unit" "$src/README.md" "$example" \
	"$src/LICENSE-MIT" "$src/LICENSE-APACHE"; do
	[ -f "$file" ] || missing="$missing $file"
done
for theme in "$themes"/*.toml; do
	[ -f "$theme" ] || missing="$missing $themes/*.toml"
	break
done
if [ -n "$missing" ]; then
	echo "install.sh: nothing installed; missing:$missing" >&2
	exit 1
fi

# put MODE DIR FILE... — install FILEs into DESTDIR/DIR, creating it.
put() {
	mode=$1
	dir=$DESTDIR$2
	shift 2
	install -d "$dir"
	install -m "$mode" "$@" "$dir"
}

for bin in $BINS; do
	put 755 "$BINDIR" "$BINSRC/$bin"
done
put 644 "$SYSTEMDUSERUNITDIR" "$overlayd_unit" "$draw_unit"
put 644 "$DATADIR/wlr-utils/themes" "$themes"/*.toml
put 644 "$DOCDIR" "$src/README.md" "$example"
put 644 "$LICENSEDIR" "$src/LICENSE-MIT" "$src/LICENSE-APACHE"
