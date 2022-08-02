#!/bin/sh

set -eu

cd "$(dirname $0)"

rm -rf /usr/local/share/rofi-bw
mkdir -p /usr/local/share
cp -r build/share/rofi-bw /usr/local/share

rm -rf /usr/local/lib/rofi-bw
mkdir -p /usr/local/lib
cp -r build/lib/rofi-bw /usr/local/lib

rm -f /usr/local/bin/rofi-bw
mkdir -p /usr/local/bin
cp -r build/bin/rofi-bw /usr/local/bin
