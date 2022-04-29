#!/bin/bash
set -euo pipefail
set -x

TANGO_CORE_VERSION="3.0.0-alpha.7"
TANGO_CORE_PLATFORM="x86_64-pc-windows-gnu"

tempdir="$(mktemp -d)"
trap 'rm -rf -- "$tempdir"' EXIT

curl -L -o "${tempdir}/tango-core.zip" "https://github.com/tangobattle/tango-core/releases/download/v${TANGO_CORE_VERSION}/tango-core-v${TANGO_CORE_VERSION}-${TANGO_CORE_PLATFORM}.zip"
rm -rf bin
mkdir bin || true
pushd bin
unzip "${tempdir}/tango-core.zip"
popd

curl -L -o "${tempdir}/ffmpeg.7z" "https://www.gyan.dev/ffmpeg/builds/ffmpeg-git-essentials.7z"
mkdir "${tempdir}/ffmpeg"
pushd "${tempdir}/ffmpeg"
7z x ../ffmpeg.7z
popd

cp "${tempdir}"/ffmpeg/ffmpeg-*-essentials_build/bin/ffmpeg.exe bin
