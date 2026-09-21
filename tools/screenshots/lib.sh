#!/usr/bin/env bash
# tools/screenshots/lib.sh
#
# Reusable helpers to drive an isolated, headless nested sway compositor and
# capture reproducible screenshots / short animations of the wlr-utils tools.
#
# Design notes:
#   * The nested compositor uses WLR_BACKENDS=headless -> virtual in-memory
#     outputs, no DRM master. It is SAFE to run next to a live session.
#   * It gets its OWN WAYLAND_DISPLAY (discovered via an exec_always that writes
#     the value to a file) and its OWN SWAYSOCK -- I3SOCK too, which swayipc reads
#     first, so a tool under test never answers from the live session.
#   * Teardown kills the nested sway by PID. We never `pkill -f` a pattern that
#     could also match this script's own command line (that self-kills).
#
# Sourced by capture.sh and the scene scripts in scenes/. Meant to run as a
# detached background script (foreground `sleep` is fine there).

set -u

# --- paths & configuration ---------------------------------------------------
SHOTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHOTS_CONF="$SHOTS_DIR/nested-sway.conf"
SHOTS_RUNTIME="${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR must be set}"
SHOTS_IPC="$SHOTS_RUNTIME/wlr-shots-ipc.sock"
SHOTS_DISPFILE="$SHOTS_RUNTIME/wlr-shots-display"
SHOTS_LOG="$SHOTS_DIR/.sway.log"

# Where wlr-utils binaries live (prefer installed; fall back to release build).
SHOTS_BIN="${SHOTS_BIN:-$HOME/.local/bin}"
# Generated media land here (served by GitHub Pages, embedded in the READMEs).
SHOTS_ASSETS="${SHOTS_ASSETS:-$(cd "$SHOTS_DIR/../.." && pwd)/docs/assets}"
SHOTS_REPO="$(cd "$SHOTS_DIR/../.." && pwd)"

# Path to an output asset, creating its directory. Args: tool basename
shots_out() { mkdir -p "$SHOTS_ASSETS/$1"; printf '%s/%s' "$SHOTS_ASSETS/$1" "$2"; }

: "${SHOTS_WIDTH:=1920}"
: "${SHOTS_HEIGHT:=1080}"
: "${SHOTS_SEAT:=seat0}"
# Locale the nested apps & wlr-utils overlays render in. Default English so the
# showcase suits the wider Wayland community; set SHOTS_LANG=fr_FR.UTF-8 for French.
: "${SHOTS_LANG:=en_US.UTF-8}"
# The demo desktop plays a clip so the live previews have something moving in
# them. Big Buck Bunny's trailer: CC-BY 3.0, served as a direct file by the
# Blender Foundation. Fetched once, trimmed to a few seconds and cached in
# vendor/ (git-ignored) — nothing binary enters the repository.
: "${SHOTS_CLIP_URL:=https://download.blender.org/peach/trailer/trailer_720p.mov}"
: "${SHOTS_CLIP_SHA256:=cb7be60feb4fd4ff5b006e35e824201a48361b7ebc2f2bbcaf3351912035561a}"
# Trim window: a stretch of bright footage, clear of the trailer's title cards.
: "${SHOTS_CLIP_START:=13.3}"
: "${SHOTS_CLIP_SECONDS:=3.2}"
# A scene that only takes a still can freeze the clip.
: "${SHOTS_MPV_PAUSE:=0}"
# How many pixels must change for shots_expect_change to call an effect visible.
: "${SHOTS_CHANGE_MIN:=200}"
SHOTS_FOOT_INI="$SHOTS_DIR/foot.ini"
# uBlock Origin Lite (unpacked, MV3) loaded into the demo browsers to keep ads
# out of the captures. Fetched into vendor/ubol by capture.sh if missing.
SHOTS_UBO="${SHOTS_UBO:-$SHOTS_DIR/vendor/ubol}"
SHOTS_CLIP="${SHOTS_CLIP:-$SHOTS_DIR/vendor/clip.mp4}"

SHOTS_SWAY_PID=""
NESTED_WAYLAND_DISPLAY=""

shots_msg() { printf '[shots] %s\n' "$*" >&2; }
shots_die() { shots_msg "ERROR: $*"; shots_stop; exit 1; }

# Resolve a wlr-utils binary: installed copy first, then target/release.
shots_tool() {
  local name="$1"
  if [ -x "$SHOTS_BIN/$name" ]; then printf '%s\n' "$SHOTS_BIN/$name"
  elif [ -x "$SHOTS_DIR/../../target/release/$name" ]; then
    printf '%s\n' "$SHOTS_DIR/../../target/release/$name"
  else printf '%s\n' "$name"; fi
}

# --- lifecycle ---------------------------------------------------------------

# Kill any stray nested sway from a previous aborted run, by PID + cmdline match
# against our private config path (safe: never matches this script).
shots_kill_stray() {
  local p cl
  for p in $(pgrep -x sway 2>/dev/null); do
    cl="$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null)"
    case "$cl" in *"$SHOTS_CONF"*) kill -9 "$p" 2>/dev/null ;; esac
  done
}

shots_start() {
  local res="${1:-${SHOTS_WIDTH}x${SHOTS_HEIGHT}}"
  SHOTS_WIDTH="${res%x*}"; SHOTS_HEIGHT="${res#*x}"
  shots_kill_stray
  rm -f "$SHOTS_DISPFILE"

  env -u DISPLAY -u WAYLAND_DISPLAY -u SWAYSOCK -u I3SOCK \
      WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
      SWAYSOCK="$SHOTS_IPC" I3SOCK="$SHOTS_IPC" \
      sway -c "$SHOTS_CONF" > "$SHOTS_LOG" 2>&1 &
  SHOTS_SWAY_PID=$!

  # Wait until the compositor publishes its WAYLAND_DISPLAY.
  local i
  for i in $(seq 1 60); do
    [ -s "$SHOTS_DISPFILE" ] && break
    kill -0 "$SHOTS_SWAY_PID" 2>/dev/null || shots_die "nested sway exited early; see $SHOTS_LOG"
    sleep 0.1
  done
  NESTED_WAYLAND_DISPLAY="$(cat "$SHOTS_DISPFILE" 2>/dev/null)"
  [ -n "$NESTED_WAYLAND_DISPLAY" ] || shots_die "could not discover nested WAYLAND_DISPLAY"
  [ -S "$SHOTS_RUNTIME/$NESTED_WAYLAND_DISPLAY" ] || shots_die "nested wayland socket missing"

  # From here on, every command in THIS process targets the nested instance.
  export WAYLAND_DISPLAY="$NESTED_WAYLAND_DISPLAY"
  export SWAYSOCK="$SHOTS_IPC" I3SOCK="$SHOTS_IPC"
  export LANG="$SHOTS_LANG" LC_ALL="$SHOTS_LANG"
  unset DISPLAY

  swaymsg "output HEADLESS-1 resolution $res" >/dev/null 2>&1
  swaymsg "output HEADLESS-1 background $SHOTS_DIR/assets/wallpaper.png fill" >/dev/null 2>&1
  shots_pointer_start
  shots_msg "nested sway up: WAYLAND_DISPLAY=$WAYLAND_DISPLAY ($res)"
}

shots_stop() {
  shots_pointer_stop
  # Tear down any Chromium windows (matched by their unique throwaway profile
  # path, which never appears in this script's own command line).
  local prof
  for prof in "${SHOTS_CHROMIUM_PROFILES[@]:-}"; do
    [ -n "$prof" ] || continue
    pkill -f -- "$prof" 2>/dev/null
  done
  [ -n "${SHOTS_SWAY_PID:-}" ] || { SHOTS_CHROMIUM_PROFILES=(); return 0; }
  swaymsg exit >/dev/null 2>&1
  sleep 0.4
  kill "$SHOTS_SWAY_PID" 2>/dev/null
  SHOTS_SWAY_PID=""
  for prof in "${SHOTS_CHROMIUM_PROFILES[@]:-}"; do
    [ -n "$prof" ] && rm -rf "$prof" 2>/dev/null
  done
  SHOTS_CHROMIUM_PROFILES=()
}

# --- scene helpers -----------------------------------------------------------

# Launch a client detached in the nested session. Args: cmd...
shots_spawn() { ( setsid "$@" >/dev/null 2>&1 < /dev/null & ) ; }

# Number of mapped windows in the nested tree whose "app_id name" matches an
# extended regex. Args: regex
shots_count_windows() {
  swaymsg -t get_tree 2>/dev/null | jq --arg p "$1" '
    [ .. | objects | select(.pid != null and .pid != 0)
      | ((.app_id // .window_properties.class // "?") + " " + (.name // ""))
      | select(test($p)) ] | length' 2>/dev/null
}

# Wait until at least N windows matching a regex are mapped, like shots_start
# waits for its WAYLAND_DISPLAY. A scene app that dies on startup otherwise goes
# unnoticed and the capture simply comes out one window short, so say so loudly
# instead of shooting a half-built set. Args: regex [count] [seconds]
shots_wait_window() {
  local pat="$1" want="${2:-1}" secs="${3:-20}" i n=0
  for i in $(seq 1 $((secs * 10))); do
    n="$(shots_count_windows "$pat")"
    [ "${n:-0}" -ge "$want" ] && return 0
    sleep 0.1
  done
  shots_msg "MISSING WINDOW: expected $want matching /$pat/, found ${n:-0} after ${secs}s"
  return 1
}

# PIDs of wlr-draw daemons attached to a given WAYLAND_DISPLAY ("" = any other
# session than ours). Args: display
shots_draw_pids() {
  local p d
  for p in $(pgrep -x wlr-draw 2>/dev/null); do
    d="$(tr '\0' '\n' < "/proc/$p/environ" 2>/dev/null | sed -n 's/^WAYLAND_DISPLAY=//p')"
    if [ -n "$1" ]; then [ "$d" = "$1" ] && printf '%s\n' "$p"
    else [ "$d" != "$WAYLAND_DISPLAY" ] && printf '%s\n' "$p"; fi
  done
}

# Start the scene's wlr-draw daemon and return only once OUR daemon answers.
#
# The control socket lives in $XDG_RUNTIME_DIR, which the nested session shares
# with the live one. A daemon already listening there — the user's own, say —
# keeps ours from binding, and every control command the scene then sends lands
# in the LIVE session: the capture comes out with no annotation on it and the
# real screen gets the commands. Refuse instead of shooting that. Args: binary
shots_draw_start() {
  local draw="$1" foreign i
  foreign="$(shots_draw_pids "" | tr '\n' ' ')"
  if [ -n "${foreign// /}" ]; then
    shots_msg "FOREIGN wlr-draw DAEMON (pid ${foreign% }) owns $XDG_RUNTIME_DIR/wlr-draw.sock; stop it before capturing"
    return 1
  fi
  shots_spawn "$draw"
  for i in $(seq 1 100); do
    if [ -n "$(shots_draw_pids "$WAYLAND_DISPLAY")" ] && [ -S "$XDG_RUNTIME_DIR/wlr-draw.sock" ]; then
      shots_settle 0.8      # the socket is bound before the overlay is mapped
      return 0
    fi
    sleep 0.1
  done
  shots_msg "NO wlr-draw DAEMON: the nested daemon never came up"
  return 1
}

# Open a styled foot terminal running a command (kept alive afterwards).
# Args: title cmd
shots_term() {
  local title="$1"; shift
  shots_spawn foot --config "$SHOTS_FOOT_INI" --title "$title" \
    -- bash -lc "$*; exec sleep 100000"
  shots_wait_window "^foot $title\$"
}

shots_settle() { sleep "${1:-0.6}"; }

# Park the cursor in the outer gap (over wallpaper, off every window) so it can't
# trigger hover tooltips/popups in the captured windows.
shots_park() { shots_cursor 7 7; }

# Launch a Chromium window (native Wayland) on a URL, with a throwaway profile,
# an ad-blocker, and a DevTools port (so we can dismiss cookie walls). Records the
# port in SHOTS_LAST_PORT for a following shots_consent.
SHOTS_CHROMIUM_PROFILES=()
SHOTS_CDP_PORT=9400
SHOTS_LAST_PORT=""
shots_chromium() {
  local url="$1" prof ext=()
  prof="$(mktemp -d)"; SHOTS_CHROMIUM_PROFILES+=("$prof")
  SHOTS_LAST_PORT=$((SHOTS_CDP_PORT++))
  [ -d "${SHOTS_UBO:-/nonexistent}" ] && ext=(--load-extension="$SHOTS_UBO")
  shots_spawn chromium --ozone-platform=wayland --no-first-run \
    --no-default-browser-check --disable-dev-shm-usage --disable-features=Translate \
    --force-prefers-color-scheme=light \
    --remote-debugging-port="$SHOTS_LAST_PORT" --remote-allow-origins='*' \
    "${ext[@]}" \
    --user-data-dir="$prof" --new-window "$url"
  # Each call opens exactly one window, so the profile count is how many
  # chromium windows the tree must hold by now.
  shots_wait_window '^chromium ' "${#SHOTS_CHROMIUM_PROFILES[@]}" 30
}

# Dismiss a cookie-consent dialog on the most recently launched Chromium (or the
# given port), deterministically — clicks an Accept button via the DevTools
# protocol, reaching into cross-origin consent iframes. No-op if there's none.
shots_consent() { python3 "$SHOTS_DIR/cdp.py" "${1:-$SHOTS_LAST_PORT}" accept >/dev/null 2>&1; }

# Switch the nested compositor to workspace N.
shots_ws() { swaymsg "workspace $1" >/dev/null 2>&1; }

# Path to the cached demo clip, fetching and trimming it on first use. The
# download is checked against a pinned digest before anything is cut from it.
# When it cannot be had, the scene keeps a moving window: a generated test
# pattern, cached under its own name so the real clip is retried next run.
shots_clip() {
  [ -s "$SHOTS_CLIP" ] && { printf '%s\n' "$SHOTS_CLIP"; return 0; }
  local src fallback="${SHOTS_CLIP%.mp4}-testpattern.mp4"
  mkdir -p "$(dirname "$SHOTS_CLIP")"
  src="$(mktemp --suffix=.mov)"
  if curl -fsSL --max-time 300 "$SHOTS_CLIP_URL" -o "$src" \
     && printf '%s  %s\n' "$SHOTS_CLIP_SHA256" "$src" | sha256sum --check --status \
     && ffmpeg -v error -y -ss "$SHOTS_CLIP_START" -t "$SHOTS_CLIP_SECONDS" -i "$src" \
          -an -c:v libx264 -crf 20 -preset slow -pix_fmt yuv420p "$SHOTS_CLIP"; then
    rm -f "$src"
    printf '%s\n' "$SHOTS_CLIP"
    return 0
  fi
  rm -f "$src" "$SHOTS_CLIP"
  shots_msg "CLIP UNAVAILABLE: could not fetch or verify $SHOTS_CLIP_URL — the demo desktop falls back to a test pattern"
  [ -s "$fallback" ] || ffmpeg -v error -y -f lavfi \
    -i "testsrc2=size=1280x720:rate=25:duration=$SHOTS_CLIP_SECONDS" \
    -c:v libx264 -crf 20 -preset slow -pix_fmt yuv420p "$fallback" || return 1
  printf '%s\n' "$fallback"
}

# Play the cached clip in a clean mpv window, looping so the window keeps moving
# for the whole scene. It is the only moving thing on the demo desktop, and a
# still picture would prove nothing about live previews. No --force-window: the
# window appears only once playback starts, so the check below means what it
# says. Args: title
shots_mpv() {
  local clip pause=()
  clip="$(shots_clip)" || { shots_msg "MISSING WINDOW: no demo clip, the video window is skipped"; return 1; }
  [ "$SHOTS_MPV_PAUSE" = 1 ] && pause=(--pause)
  shots_spawn mpv --no-audio --loop-file=inf "${pause[@]}" \
    --no-terminal --title="$1" "$clip"
  shots_wait_window '^mpv ' 1 20
}

# A realistic desktop for the switcher / chooser / exposé scenes: real GUI apps
# (three browsers, a video, a calculator, two system monitors) spread across SIX
# workspaces, so the exposé/switcher reveal windows from the other workspaces.
# Browsers/video are slow to paint, so this settles generously. Leaves the
# session on workspace 1.
shots_rich_desktop() {
  # ws2: phoronix on its own, then dismiss its cookie wall. The consent dialog is
  # centred and a fixed pixel size; its "Accept" button sits just below centre.
  shots_ws 2
  shots_chromium "https://www.phoronix.com"
  shots_settle 10
  shots_consent          # deterministically accept the cookie wall (CDP)
  shots_settle 1.5
  # ws4: the video, in a clean mpv window.
  shots_ws 4
  shots_mpv "Big Buck Bunny"
  shots_settle 1.0
  # ws5/ws6: two terminals that redraw on their own, so several previews move and
  # not just the video. btop runs off our own config, which drops its process
  # list — a published capture has no business showing the machine's command
  # lines. cmatrix carries no data at all.
  shots_ws 5
  shots_term "btop" "btop -c '$SHOTS_DIR/btop.conf'"
  shots_ws 6
  shots_term "cmatrix" "cmatrix -ab"
  # ws3: a second light page + a calculator (non-browser variety, also light).
  # The API docs rather than the crates.io page: the latter embeds the README's
  # demo video, whose loop made the live thumbnail of that window flicker.
  shots_ws 3
  shots_chromium "https://docs.rs/wlr-capture/latest/wlr_capture/"
  shots_settle 0.8
  shots_spawn galculator
  shots_wait_window '^galculator '
  shots_settle 0.6
  # ws1 (the visible one): the repo on GitHub.
  shots_ws 1
  shots_chromium "https://github.com/sjourdois/wlr-utils"
  shots_settle 9   # let the pages + the video finish loading and painting
  shots_park       # neutral cursor so live thumbnails carry no hover tooltips
}

# A rich desktop on the VISIBLE workspace (several light GUI windows tiled), for
# the overlays that capture/freeze the on-screen content (shot, peek, draw,
# mirror). Leaves the cursor parked.
shots_visible_desktop() {
  # Layout: github | (video stacked ABOVE calculator) | phoronix. We open the two
  # browsers first, then focus back to github so the video lands BETWEEN them, and
  # splitv so the calculator stacks under the video.
  shots_chromium "https://github.com/sjourdois/wlr-utils"
  shots_settle 9
  shots_chromium "https://www.phoronix.com"
  shots_settle 10
  shots_consent
  shots_settle 1.0
  swaymsg "focus left" >/dev/null 2>&1   # back to github
  shots_mpv "Big Buck Bunny"
  shots_settle 3.0                        # video maps between github and phoronix
  swaymsg "splitv" >/dev/null 2>&1        # the calculator goes BELOW the video
  shots_spawn galculator
  shots_wait_window '^galculator '
  shots_settle 1.0
  shots_park
}

# A busy desktop of five windows with distinct, colourful content — shared by
# the switcher/chooser scenes. Avoids size-sensitive TUIs (btop) that warn when
# tiled small. Leaves the windows running.
shots_demo_windows() {
  local src="$SHOTS_REPO/crates/wlr-capture/src/lib.rs"
  shots_term "git"    "git -C '$SHOTS_REPO' -c color.ui=always log --graph --oneline --decorate -30 | head -45"
  shots_settle 0.3
  shots_term "lib.rs" "batcat --paging=never --style=numbers,grid --color=always --line-range :70 '$src' 2>/dev/null"
  shots_settle 0.3
  shots_term "tree"   "tree -C -L 3 --dirsfirst '$SHOTS_REPO/crates' 2>/dev/null | head -50"
  shots_settle 0.3
  shots_term "colors" "for i in \$(seq 0 255); do printf '\\033[48;5;%sm  \\033[0m' \"\$i\"; [ \$(((i+1)%16)) -eq 0 ] && echo; done; echo"
  shots_settle 0.3
  shots_term "log"    "batcat --paging=never --style=numbers --color=always --line-range :70 '$SHOTS_REPO/CHANGELOG.md' 2>/dev/null"
  shots_settle 1.8
}

# --- input injection ---------------------------------------------------------
#
# A headless seat with no input devices has no pointer capability, so sway's
# `seat cursor` IPC does not deliver wl_pointer events. We instead drive a
# persistent virtual pointer (shots-pointer) which creates the capability and
# keeps position stable. Keyboard goes through wtype (virtual-keyboard).

SHOTS_POINTER="${SHOTS_POINTER:-$SHOTS_DIR/pointer/target/release/shots-pointer}"
SHOTS_PFIFO=""
SHOTS_PFD=""

shots_pointer_start() {
  [ -x "$SHOTS_POINTER" ] || shots_die "injector missing; run: (cd $SHOTS_DIR/pointer && cargo build --release)"
  SHOTS_PFIFO="$(mktemp -u)"; mkfifo "$SHOTS_PFIFO"
  setsid "$SHOTS_POINTER" < "$SHOTS_PFIFO" >/dev/null 2>&1 &
  exec {SHOTS_PFD}> "$SHOTS_PFIFO"        # hold the write end open
  sleep 0.4                               # let the pointer capability appear
}

shots_pointer_stop() {
  [ -n "$SHOTS_PFD" ] || return 0
  printf 'quit\n' >&"$SHOTS_PFD" 2>/dev/null
  exec {SHOTS_PFD}>&- 2>/dev/null
  rm -f "$SHOTS_PFIFO"; SHOTS_PFD=""; SHOTS_PFIFO=""
}

shots_pcmd()   { printf '%s\n' "$*" >&"$SHOTS_PFD"; }
# Hold/release a key on the virtual keyboard by evdev code (held until released):
# Shift_L=42, Ctrl_L=29, Alt_L=56, Space=57, Return=28, Escape=1.
shots_kdown()  { shots_pcmd "key ${1:-42} 1"; }
shots_kup()    { shots_pcmd "key ${1:-42} 0"; }
# Tap Tab on the virtual keyboard (evdev 15). The injector's keyboard drives the
# exposé/strip selection where wtype does not.
shots_tab()    { shots_kdown 15; sleep 0.04; shots_kup 15; }
shots_cursor() { shots_pcmd "abs $1 $2 $SHOTS_WIDTH $SHOTS_HEIGHT"; }
shots_press()  { shots_pcmd "btn ${1:-l} 1"; }
shots_release(){ shots_pcmd "btn ${1:-l} 0"; }
shots_click()  { shots_press "${1:-l}"; sleep 0.1; shots_release "${1:-l}"; }
shots_scroll() { shots_pcmd "scroll ${1:-v} ${2:-1}"; }

# Glide the cursor smoothly from (x1,y1) to (x2,y2) over N steps (no button).
# Use this instead of a bare shots_cursor jump when the motion itself is on screen
# (spotlight, loupe), so it reads as a smooth move rather than a teleport.
shots_glide() {
  local x1=$1 y1=$2 x2=$3 y2=$4 n=${5:-22} i
  for i in $(seq 1 "$n"); do
    shots_cursor "$(( x1 + (x2 - x1) * i / n ))" "$(( y1 + (y2 - y1) * i / n ))"
    sleep 0.035
  done
}

# Press at (x1,y1), drag through to (x2,y2) in N steps, release.
shots_drag() {
  local x1=$1 y1=$2 x2=$3 y2=$4 n=${5:-14} i
  shots_cursor "$x1" "$y1"; sleep 0.12; shots_press; sleep 0.05
  for i in $(seq 1 "$n"); do
    shots_cursor "$(( x1 + (x2 - x1) * i / n ))" "$(( y1 + (y2 - y1) * i / n ))"
    sleep 0.03
  done
  sleep 0.1; shots_release
}

# wtype spins up a fresh virtual keyboard each call and the compositor can drop
# the very first keystroke while it installs the keymap. Absorb that drop with a
# harmless leading modifier tap IN THE SAME invocation (a separate wtype call
# would create another keyboard and lose the first real key again).
shots_type() { wtype -k Shift_L "$*" 2>/dev/null; }
shots_key()  { wtype -k Shift_L -k "$1" 2>/dev/null; }

# --- capture -----------------------------------------------------------------

# Grab the virtual output to a PNG, including the cursor (-c). Args: outfile [output]
shots_grab() { grim -c -o "${2:-HEADLESS-1}" "$1" 2>/dev/null; }

# Run an action between two grabs and warn when the screen barely moved. For the
# effects no IPC can confirm -- a pointer hover, whose coordinate is tied to a
# layout sway cannot report, so a redesigned tool would quietly yield a still
# with nothing highlighted. The grabs leave the cursor OUT: moving it is itself a
# patch of changed pixels and would satisfy any threshold on its own. What is
# left is the effect (a tile highlight is a border hundreds of pixels long)
# against the desktop's own churn (a blinking caret, a live video thumbnail), so
# the verdict is a threshold, not equality. Args: label cmd...
shots_expect_change() {
  local label="$1"; shift
  local before after changed
  before="$(mktemp --suffix=.png)"; after="$(mktemp --suffix=.png)"
  grim -o HEADLESS-1 "$before" 2>/dev/null
  "$@"
  grim -o HEADLESS-1 "$after" 2>/dev/null
  changed="$(magick compare -metric AE "$before" "$after" null: 2>&1 | awk '{printf "%d", $1}')"
  rm -f "$before" "$after"
  [ "${changed:-0}" -ge "$SHOTS_CHANGE_MIN" ] && return 0
  shots_msg "NO VISIBLE CHANGE: $label moved ${changed:-0} px (< $SHOTS_CHANGE_MIN)"
  return 1
}

# Capture a sequence of frames while a driver function runs, then assemble a
# looping animation in two formats: MP4 (the primary one) and GIF (maximum
# compatibility, embedded in the READMEs). Args: basename fps driver_fn
# The driver function is invoked with the frame directory as $1 and should
# return after issuing all its input; frames are grabbed in parallel.
shots_record() {
  local out="$1" fps="$2" driver="$3"
  local dir; dir="$(mktemp -d)"
  local period; period="$(awk "BEGIN{printf \"%.3f\", 1/$fps}")"
  local n=0
  ( # frame grabber: keep grabbing until the marker file appears
    while [ ! -e "$dir/.stop" ]; do
      grim -c -o HEADLESS-1 "$(printf '%s/f%04d.png' "$dir" "$n")" 2>/dev/null
      n=$((n+1)); sleep "$period"
    done
  ) &
  local gpid=$!
  "$driver" "$dir"
  touch "$dir/.stop"; wait "$gpid" 2>/dev/null

  # H.264 MP4 — the primary format: clean video, no palette/disposal artifacts.
  # Even dimensions + yuv420p for broad playback; +faststart for web streaming.
  ffmpeg -y -framerate "$fps" -pattern_type glob -i "$dir/f*.png" \
    -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2:flags=lanczos,format=yuv420p" \
    -c:v libx264 -crf 20 -preset medium -movflags +faststart "$out.mp4" >/dev/null 2>&1
  # GIF — full frames (no transdiff) so simple viewers (feh) don't drift; a global
  # palette keeps it stable. Embedded in the READMEs (crates.io can't play MP4).
  ffmpeg -y -framerate "$fps" -pattern_type glob -i "$dir/f*.png" \
    -gifflags -transdiff \
    -vf "scale='min(1280,iw)':-2:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse=dither=sierra2_4a" \
    "$out.gif" >/dev/null 2>&1
  rm -rf "$dir"
}
