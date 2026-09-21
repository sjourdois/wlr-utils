#!/usr/bin/env bash
# Scene: wlr-draw presenter spotlight — hold Shift to darken the screen except a
# flashlight that follows the cursor, then pose a fixed spotlight on a window.
# Produces docs/assets/wlr-draw/spotlight.{mp4,gif,png}.
set -u
cd "$(dirname "$0")/.."
. ./lib.sh

shots_start 2560x1440
shots_visible_desktop

DRAW="$(shots_tool wlr-draw)"
shots_draw_start "$DRAW" || shots_die "no wlr-draw daemon of our own"
"$DRAW" on >/dev/null 2>&1
shots_settle 0.6

present_demo() {
  shots_cursor 430 520; shots_settle 0.6
  # Hold Shift -> flashlight; glide it smoothly across the windows.
  shots_kdown 42; shots_settle 0.9
  shots_glide 430 520 1270 360 30          # github -> the video (top-middle)
  shots_settle 0.6
  shots_scroll v 3; shots_settle 0.6       # widen the flashlight
  shots_glide 1270 360 1270 1080 26        # -> the calculator (down the column)
  shots_settle 0.7
  shots_glide 1270 1080 1850 400 32        # -> the article column (right)
  shots_settle 0.6
  # Still holding Shift, pose a tight spotlight on a few articles. Two choices here,
  # both deliberate. It goes on phoronix rather than GitHub, because a spotlight only
  # reads when the lit window is a bright one — lighting GitHub's dark theme leaves
  # the lit zone as dark as the veiled surround. And it covers about three articles
  # rather than the whole column, so the veil's edge cuts across the page instead of
  # landing on a window border, where it would just look like the window is focused.
  # It spans the column's full width on purpose: inset sides would slice the
  # headlines mid-word. The top and bottom edges carry the effect — they fall on the
  # article rules at y~322 and ~805, measured on the rendered page.
  "$DRAW" tool rect >/dev/null 2>&1
  shots_drag 1700 330 2552 810 18
  shots_settle 0.6
  shots_kup 42                             # release Shift; the posed spotlight stays
  shots_settle 1.2
}
shots_record "$(shots_out wlr-draw spotlight)" 12 present_demo
shots_settle 0.3
shots_grab "$(shots_out wlr-draw spotlight.png)"

"$DRAW" quit >/dev/null 2>&1
shots_stop
echo "[draw-present] done -> $SHOTS_ASSETS/wlr-draw/spotlight.*"
