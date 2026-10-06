#!/bin/sh
# Install wlr-utils: the six binaries, the systemd user units, the themes, the README, the
# example configuration and the licences. It runs from a source tree, after
# `cargo build --release -p wlr-utils`, or from the release archive, where everything sits
# next to it.
#
#   sh packaging/install.sh                                into /usr/local
#   DESTDIR="$pkgdir" PREFIX=/usr sh packaging/install.sh  for a package
#
# Each directory can be set on its own: BINDIR, DATADIR, SYSTEMDUSERUNITDIR, DOCDIR,
# LICENSEDIR. BINSRC is where the built binaries are.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
if [ -f "$here/../Cargo.toml" ]; then
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
SYSTEMDUSERUNITDIR=${SYSTEMDUSERUNITDIR:-$PREFIX/lib/systemd/user}
DOCDIR=${DOCDIR:-$DATADIR/doc/wlr-utils}
LICENSEDIR=${LICENSEDIR:-$DATADIR/licenses/wlr-utils}

# put MODE DIR FILE... — install FILEs into DESTDIR/DIR, creating it.
put() {
	mode=$1
	dir=$DESTDIR$2
	shift 2
	install -d "$dir"
	install -m "$mode" "$@" "$dir"
}

for bin in wlr-chooser wlr-switcher wlr-overlayd wlr-peek wlr-shot wlr-draw; do
	put 755 "$BINDIR" "$BINSRC/$bin"
done
put 644 "$SYSTEMDUSERUNITDIR" "$overlayd_unit" "$draw_unit"
put 644 "$DATADIR/wlr-utils/themes" "$themes"/*.toml
put 644 "$DOCDIR" "$src/README.md" "$example"
put 644 "$LICENSEDIR" "$src/LICENSE-MIT" "$src/LICENSE-APACHE"
