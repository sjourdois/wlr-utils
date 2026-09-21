#!/usr/bin/env bash
# Scene: wlr-peek — the overlay inspectors, as short videos:
#   color  — the colour picker (pipette) + loupe, sweeping the desktop
#   loupe  — the standalone magnifier, panning and zooming
# Produces docs/assets/wlr-peek/{color,loupe}.{mp4,gif,apng,png}.
set -u
cd "$(dirname "$0")/.."
. ./lib.sh

shots_start 2560x1440
shots_visible_desktop

PEEK="$(shots_tool wlr-peek)"

# --- colour picker (pipette): call at surfaces whose colour is worth reading ---
# The sweep deliberately avoids the video: film is smooth and desaturated, so both
# the swatch and the magnified preview come out a flat beige that shows nothing.
color_demo() {
  setsid "$PEEK" color >/dev/null 2>&1 < /dev/null &
  P_PID=$!
  shots_settle 2.2
  shots_cursor 504 328;   shots_settle 1.4        # GitHub's green Code button
  shots_glide 504 328 637 623 26;   shots_settle 1.4   # the blue topic pills below
  shots_glide 637 623 1270 1150 36; shots_settle 1.4   # the calculator keypad
  shots_glide 1270 1150 2080 480 36; shots_settle 1.4  # phoronix (right)
  shots_settle 0.6
}
shots_record "$(shots_out wlr-peek color)" 12 color_demo
shots_settle 0.2
shots_cursor 504 328; shots_settle 0.5          # the still poses on the green button
shots_grab "$(shots_out wlr-peek color.png)"
shots_key Escape
kill "${P_PID:-0}" 2>/dev/null
shots_settle 1.0

# --- standalone magnifier: pan slowly across windows, scroll to zoom ----------
loupe_demo() {
  setsid "$PEEK" loupe >/dev/null 2>&1 < /dev/null &
  P_PID=$!
  shots_settle 2.2
  shots_cursor 430 420;  shots_settle 1.4         # the GitHub page (left)
  shots_scroll v 3;      shots_settle 1.2         # zoom in
  # The middle stop is the calculator, not the video above it: magnifying film
  # shows nothing (it has no fine detail), where crisp key labels read as magnified.
  shots_glide 430 420 1270 1150 36; shots_settle 1.4  # pan to the calculator (middle)
  shots_scroll v 2;      shots_settle 1.2         # zoom more
  shots_glide 1270 1150 2080 500 36; shots_settle 1.4 # pan to phoronix (right)
  shots_settle 0.6
}
shots_record "$(shots_out wlr-peek loupe)" 12 loupe_demo
shots_settle 0.2
shots_grab "$(shots_out wlr-peek loupe.png)"
shots_key Escape
kill "${P_PID:-0}" 2>/dev/null
shots_settle 0.8

shots_stop
echo "[peek] done -> $SHOTS_ASSETS/wlr-peek/"
