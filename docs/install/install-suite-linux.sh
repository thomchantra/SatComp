#!/usr/bin/env bash
# SatComp suite @VER@ installer for Linux: copies every VST3 and CLAP plug-in in this folder into your plug-in folders.
#   bash install.sh            install for you (~/.vst3 and ~/.clap)
#   bash install.sh --system   install for all users (/usr/local/lib/vst3 and /usr/local/lib/clap, uses sudo)
#   bash install.sh --uninstall   remove what this script installed (add --system for the all-users copy)
set -e
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [ "$1" = "--system" ] || [ "$2" = "--system" ]; then
  vst3_dir=/usr/local/lib/vst3; clap_dir=/usr/local/lib/clap; sudo=sudo
else
  vst3_dir="$HOME/.vst3"; clap_dir="$HOME/.clap"; sudo=
fi

shopt -s nullglob
vst3s=("$here"/VST3/*.vst3)
claps=("$here"/CLAP/*.clap)
[ ${#vst3s[@]} -gt 0 ] || { echo "Run this script from the unzipped folder (no VST3/*.vst3 found)."; exit 1; }

if [ "$1" = "--uninstall" ] || [ "$2" = "--uninstall" ]; then
  for p in "${vst3s[@]}"; do $sudo rm -rf "$vst3_dir/$(basename "$p")"; done
  for p in "${claps[@]}"; do $sudo rm -f "$clap_dir/$(basename "$p")"; done
  echo "SatComp suite removed from $vst3_dir and $clap_dir"
  exit 0
fi

$sudo mkdir -p "$vst3_dir" "$clap_dir"
for p in "${vst3s[@]}"; do
  $sudo rm -rf "$vst3_dir/$(basename "$p")"
  $sudo cp -r "$p" "$vst3_dir/"
  echo "  VST3  $vst3_dir/$(basename "$p")"
done
for p in "${claps[@]}"; do
  $sudo cp "$p" "$clap_dir/"
  echo "  CLAP  $clap_dir/$(basename "$p")"
done
echo "SatComp suite @VER@ installed. Rescan plug-ins in your host (REAPER: Options > Preferences > Plug-ins > VST/CLAP > Re-scan)."
