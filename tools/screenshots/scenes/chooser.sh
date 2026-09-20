#!/usr/bin/env bash
# Scene: wlr-chooser — the graphical window & screen picker for the screencast
# portal. A still only (the picker is essentially static, and an animated clip
# would duplicate wlr-switcher's card). Produces docs/assets/wlr-chooser/picker.png.
set -u
cd "$(dirname "$0")/.."
. ./lib.sh

shots_start 2560x1440
# A still needs no motion, and a desktop that moves on its own would drown the
# hover check below in churn.
SHOTS_MPV_PAUSE=1
shots_rich_desktop

CH="$(shots_tool wlr-chooser)"
setsid "$CH" --both >/dev/null 2>&1 < /dev/null &
CH_PID=$!
shots_settle 2.8
# Hover a card so one tile reads as highlighted. The coordinate is tied to the
# picker's own layout, which sway cannot report, so confirm it lit something up.
hover_tile() { shots_cursor 1280 560; shots_settle 0.6; }
shots_expect_change "hovering a chooser tile at 1280 560" hover_tile
shots_grab "$(shots_out wlr-chooser picker.png)"
kill "$CH_PID" 2>/dev/null
shots_settle 0.6

shots_stop
echo "[chooser] done -> $SHOTS_ASSETS/wlr-chooser/"
