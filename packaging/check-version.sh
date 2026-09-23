#!/bin/sh
# Usage: check-version.sh DIR TAG
# Fail unless every wlr-utils binary in DIR reports exactly TAG's release version
# (`wlr-shot 1.9.0` for `v1.9.0`) — not a `git describe` form.
set -eu
dir=$1
want=${2#v}
status=0
for bin in wlr-chooser wlr-switcher wlr-overlayd wlr-peek wlr-shot wlr-draw; do
	got=$("$dir/$bin" --version) || got="(failed to run)"
	if [ "$got" = "$bin $want" ]; then
		echo "$got"
	else
		echo "::error::$bin reports '$got', expected '$bin $want'"
		status=1
	fi
done
exit "$status"
