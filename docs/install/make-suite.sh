#!/usr/bin/env bash
# Builds the all-in-one release files from the per-plug-in zips the bundle job made.
#   make-suite.sh <platform> <tag> <dir with the per-plug-in zips> <output dir>
# platform: macOS-universal | Windows-x64 | Linux-x64. Writes SatComp-Suite-<tag>-<platform>.zip (all four VST3 / CLAP, one joint
# manual, a suite INSTALL.txt, install.sh on Linux), SatComp-Suite-Manual-<tag>.pdf (the joint manual: the per-plug-in manuals
# joined in suite order) and, on macOS, SatComp-Suite-<tag>-macOS-universal.pkg (installs for all users).
# Needs pdfunite (poppler) for the joint manual.
set -euo pipefail
export COPYFILE_DISABLE=1
plat="$1"; tag="$2"; zips="$3"; out="$4"
here="$(cd "$(dirname "$0")" && pwd)"
case "$plat" in macOS*) os=macOS ;; Linux*) os=Linux ;; *) os=Windows ;; esac
work="$(mktemp -d)"; dist="$work/dist"; mkdir -p "$dist" "$out"
n=0
for z in "$zips"/*-"$plat".zip; do
  case "$(basename "$z")" in SatComp-Suite*) continue ;; esac
  unzip -q -o "$z" -d "$dist"; n=$((n + 1))
done
[ "$n" -gt 0 ] || { echo "no per-plug-in zips for $plat in $zips"; exit 1; }
rm -f "$dist/INSTALL.txt" "$dist/install.sh"
# joint manual: the per-plug-in manuals (each zip carries its own) joined in suite order, replacing them in the suite zip
manuals=()
for d in SatComp SatXComp SatLim SatDiff; do
  m=("$dist/$d"-Manual-v*.pdf)
  [ -f "${m[0]}" ] && manuals+=("${m[0]}")
done
if [ "${#manuals[@]}" -gt 0 ]; then
  command -v pdfunite >/dev/null || { echo "pdfunite (poppler) is needed to join the manuals"; exit 1; }
  pdfunite "${manuals[@]}" "$work/manual.pdf"
  rm -f "$dist"/*-Manual-v*.pdf
  cp "$work/manual.pdf" "$dist/SatComp-Suite-Manual-${tag}.pdf"
  cp "$work/manual.pdf" "$(cd "$out" && pwd)/SatComp-Suite-Manual-${tag}.pdf"
fi
sed -e "s/@VER@/${tag}/g" "$here/SUITE-INSTALL-${os}.txt" > "$dist/INSTALL.txt"
if [ "$os" = Linux ]; then
  sed -e "s/@VER@/${tag}/g" "$here/install-suite-linux.sh" > "$dist/install.sh"
  chmod +x "$dist/install.sh"
fi
zipf="$(cd "$out" && pwd)/SatComp-Suite-${tag}-${plat}.zip"
rm -f "$zipf"
(cd "$dist" && zip -qr "$zipf" .)
echo "wrote $zipf ($n plug-ins)"
if [ "$os" = macOS ]; then
  root="$work/pkgroot/Library/Audio/Plug-Ins"
  mkdir -p "$root/VST3" "$root/CLAP"
  cp -R "$dist"/VST3/*.vst3 "$root/VST3/"
  cp -R "$dist"/CLAP/*.clap "$root/CLAP/"
  pkg="$(cd "$out" && pwd)/SatComp-Suite-${tag}-${plat}.pkg"
  pkgbuild --root "$work/pkgroot" --identifier com.thomchantra.satcomp.suite --version "${tag#v}" --install-location / "$pkg" >/dev/null
  echo "wrote $pkg"
fi
rm -rf "$work"
