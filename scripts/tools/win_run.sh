#!/usr/bin/env bash
# win_run.sh <out-dir> <seconds> <script> [game args...] — the windowed game
# on its own Xvfb display, driven by a key script (card BLKB's acceptance run;
# reusable for any check of the WINDOWED path). Never the user's
# desktop: WAYLAND_DISPLAY unset, a private runtime dir, a scratch profile
# (OPENBF_PROFILE), no sound device (an empty ALSA config). Hold ci.lock:
#   flock engine/artifacts/loop/ci.lock scripts/tools/win_run.sh ...
# GAME=<binary> (default engine/target/release/game) and ENGINE=<dir to run in>
# (default <repo>/engine) pick the build; OPENBF_DEV=1 is set.
#   script: a shell snippet run with $D (the display number) and $XK (xkey.py),
#           e.g. 'sleep 30; python3 $XK $D key 0xffc6; sleep 5; python3 $XK $D close'
# Extra environment for the game comes from the caller's GAME_ENV (a string of
# NAME=value words).
set -u
OUT=$1; SECS=$2; SCRIPT=$3; shift 3
ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
ENGINE=${ENGINE:-$ROOT/engine}
GAME=${GAME:-$ROOT/engine/target/release/game}
XK=$ROOT/scripts/tools/xkey.py
mkdir -p "$OUT/run"
D=141
while [ -e /tmp/.X11-unix/X$D ]; do D=$((D + 1)); done
Xvfb :$D -ac -screen 0 1280x720x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
XPID=$!
trap 'kill $XPID 2>/dev/null; wait $XPID 2>/dev/null' EXIT
for _ in $(seq 50); do [ -e /tmp/.X11-unix/X$D ] && break; sleep 0.1; done
: >"$OUT/empty_asound.conf"
cd "$ENGINE" || exit 2
# shellcheck disable=SC2086
env -u WAYLAND_DISPLAY DISPLAY=:$D XDG_RUNTIME_DIR="$OUT/run" \
  ALSA_CONFIG_PATH="$OUT/empty_asound.conf" OPENBF_PROFILE="$OUT/profile.ron" \
  OPENBF_DEV=1 ${GAME_ENV:-} \
  timeout "$SECS" "$GAME" "$@" >"$OUT/game.log" 2>&1 &
GPID=$!
export D XK
bash -c "$SCRIPT" >"$OUT/script.log" 2>&1
wait $GPID
echo "game exit $?" >>"$OUT/game.log"
