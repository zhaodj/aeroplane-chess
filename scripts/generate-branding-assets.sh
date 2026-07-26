#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
icon_svg="$project_dir/assets/branding/aeroplane-chess-icon.svg"
round_icon_svg="$project_dir/assets/branding/aeroplane-chess-icon-round.svg"

if ! command -v rsvg-convert >/dev/null 2>&1; then
  echo "rsvg-convert is required to export branding assets" >&2
  exit 1
fi

render_png() {
  local size="$1"
  local output="$2"
  rsvg-convert --format png --width "$size" --height "$size" "$icon_svg" --output "$output"
}

render_png 1024 "$project_dir/assets/branding/aeroplane-chess-logo.png"

render_png 16 "$project_dir/web/favicon-16x16.png"
render_png 32 "$project_dir/web/favicon-32x32.png"
render_png 48 "$project_dir/web/favicon-48x48.png"
render_png 64 "$project_dir/web/favicon-64x64.png"
render_png 180 "$project_dir/web/apple-touch-icon.png"
render_png 192 "$project_dir/web/android-chrome-192x192.png"
render_png 512 "$project_dir/web/android-chrome-512x512.png"
python3 "$project_dir/scripts/generate-favicon-ico.py" "$project_dir/web/favicon.ico"

android_densities="mipmap-mdpi:48 mipmap-hdpi:72 mipmap-xhdpi:96 mipmap-xxhdpi:144 mipmap-xxxhdpi:192"

for density_size in $android_densities; do
  density="${density_size%%:*}"
  size="${density_size##*:}"
  output_dir="$project_dir/platforms/android/app/src/main/res/$density"
  render_png "$size" "$output_dir/ic_launcher.png"
  rsvg-convert --format png --width "$size" --height "$size" "$round_icon_svg" --output "$output_dir/ic_launcher_round.png"
done
