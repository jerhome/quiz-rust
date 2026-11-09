#!/usr/bin/env bash
# Renders the PNG/ICO icons with ImageMagick (its SVG renderer ignores gradients, so the
# master is drawn with the same geometry as quiz-rust.svg). Usage: ./render.sh
set -euo pipefail
cd "$(dirname "$0")"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

master() { # master <variant> <out>
  local variant="$1" out="$2" shape ques extra
  if [ "$variant" = small ]; then
    shape="roundrectangle 182,170 842,700 150,150 polygon 290,660 262,850 460,700"
    ques="M 380 380 C 380 300 440 252 512 252 C 590 252 646 300 646 372 C 646 432 610 458 570 484 C 540 504 512 524 512 580"
    qw=100; dot="512,676 512,620"; badge=""
  else
    shape="roundrectangle 232,230 792,670 130,130 polygon 300,650 270,790 440,680"
    ques="M 398 404 C 398 338 448 298 512 298 C 580 298 626 338 626 398 C 626 446 596 468 562 490 C 530 511 512 530 512 572"
    qw=66; dot="512,646 512,606"; badge=1
  fi
  # background: diagonal gradient clipped to a rounded square
  convert -size 1500x1500 gradient:'#9A8CF9-#5042CC' -rotate -28 +repage -gravity center -crop 1024x1024+0+0 +repage "$tmp/bg.png"
  convert -size 1024x1024 xc:black -fill white -draw "roundrectangle 0,0 1023,1023 226,226" "$tmp/mask.png"
  convert "$tmp/bg.png" "$tmp/mask.png" -alpha off -compose CopyOpacity -composite "$tmp/bg_r.png"
  # speech bubble (stroke widens the shapes and rounds the tail corners)
  convert -size 1024x1024 xc:black -fill white -stroke white -strokewidth 40 -draw "stroke-linejoin round $shape" "$tmp/bubble_mask.png"
  convert -size 1024x1024 gradient:'#FFFFFF-#D9DAF0' "$tmp/bubble_mask.png" -alpha off -compose CopyOpacity -composite "$tmp/bubble.png"
  convert "$tmp/bg_r.png" "$tmp/bubble.png" -compose Over -composite "$tmp/step.png"
  # question mark
  set -- $dot
  convert "$tmp/step.png" -fill none -stroke '#5042CC' -strokewidth "$qw" \
    -draw "stroke-linecap round stroke-linejoin round path '$ques'" -stroke none -fill '#5042CC' -draw "circle $1 $2" "$tmp/q.png"
  if [ -n "$badge" ]; then
    convert "$tmp/q.png" -fill '#34D399' -stroke 'rgba(255,255,255,0.35)' -strokewidth 8 -draw "circle 812,232 812,148" \
      -fill none -stroke '#0E3B2C' -strokewidth 26 -draw "stroke-linecap round stroke-linejoin round polyline 774,234 802,262 852,202" "$tmp/q.png"
  fi
  # subtle inner rim
  convert "$tmp/q.png" -fill none -stroke 'rgba(255,255,255,0.16)' -strokewidth 4 -draw "roundrectangle 6,6 1017,1017 220,220" "$out"
}

master main "$tmp/main.png"
master small "$tmp/small.png"
for s in 16 24 32; do
  convert "$tmp/small.png" -filter Lanczos -resize ${s}x${s} -unsharp 0x0.4 "quiz-rust-$s.png"
done
for s in 48 64 128 256 512; do
  convert "$tmp/main.png" -filter Lanczos -resize ${s}x${s} "quiz-rust-$s.png"
done
convert quiz-rust-16.png quiz-rust-24.png quiz-rust-32.png quiz-rust-48.png quiz-rust-64.png quiz-rust-128.png quiz-rust-256.png quiz-rust.ico
