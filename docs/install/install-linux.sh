#!/usr/bin/env bash
# @NAME@ @VER@ installer for Linux: copies the VST3 and CLAP plug-ins into your plug-in folders.
#   bash install.sh            install for you (~/.vst3 and ~/.clap)
#   bash install.sh --system   install for all users (/usr/local/lib/vst3 and /usr/local/lib/clap, uses sudo)
#   bash install.sh --uninstall   remove what this script installed
set -e
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
plugin="@PLUGIN@"
name="@NAME@"

if [ "$1" = "--system" ]; then
  vst3_dir=/usr/local/lib/vst3; clap_dir=/usr/local/lib/clap; sudo=sudo
else
  vst3_dir="$HOME/.vst3"; clap_dir="$HOME/.clap"; sudo=
fi

if [ "$1" = "--uninstall" ] || [ "$2" = "--uninstall" ]; then
  $sudo rm -rf "$vst3_dir/$plugin.vst3" "$clap_dir/$plugin.clap"
  echo "$name removed from $vst3_dir and $clap_dir"
  exit 0
fi

[ -d "$here/VST3/$plugin.vst3" ] || { echo "Run this script from the unzipped folder (VST3/$plugin.vst3 not found)."; exit 1; }

$sudo mkdir -p "$vst3_dir" "$clap_dir"
$sudo rm -rf "$vst3_dir/$plugin.vst3" "$clap_dir/$plugin.clap"
$sudo cp -r "$here/VST3/$plugin.vst3" "$vst3_dir/"
$sudo cp "$here/CLAP/$plugin.clap" "$clap_dir/"

echo "$name @VER@ installed:"
echo "  VST3  $vst3_dir/$plugin.vst3"
echo "  CLAP  $clap_dir/$plugin.clap"
echo "Rescan plug-ins in your host (REAPER: Options > Preferences > Plug-ins > VST/CLAP > Re-scan)."
