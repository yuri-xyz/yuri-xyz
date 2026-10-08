# Renders each scene's loop and encodes one animated WebP per theme.
# Usage: render [OUT_DIR [SCENE...]]   (defaults: ./assets, every scene)
#
# Frame pacing: 150 frames at 30 fps is a 5 s seamless loop. WebP stores
# per-frame delays in milliseconds, so 33 ms is as close to 30 fps as the
# format allows; the loop itself is closed by the frame count, not by time.

out=${1:-assets}
shift || true
scenes=("$@")
if (( ${#scenes[@]} == 0 )); then
  mapfile -t scenes < <(profile-art --list)
fi
frames=150
delay_ms=33

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

encode() {
  local scene=$1 theme=$2 pngs
  shopt -s nullglob
  pngs=("$work/$scene/$theme"/*.png)
  shopt -u nullglob
  if (( ${#pngs[@]} != frames )); then
    echo "expected $frames $scene/$theme frames, found ${#pngs[@]}" >&2
    return 1
  fi
  # img2webp is single-threaded; -m 4 keeps encoding under a minute.
  nice -n 10 img2webp -loop 0 -lossy -q 80 -m 4 -d "$delay_ms" \
    "${pngs[@]}" -o "$out/$scene-$theme.webp"
}

mkdir -p "$out"
for scene in "${scenes[@]}"; do
  nice -n 10 profile-art "$scene" --frames "$frames" "$work/$scene"
  encode "$scene" dark &
  dark=$!
  encode "$scene" light &
  light=$!
  wait "$dark"
  wait "$light"
done
