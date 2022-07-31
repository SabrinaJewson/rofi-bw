#!/bin/sh

set -eu

cd "$(dirname $0)"

mkdir -p /usr/local/share/rofi-bw
cp resources/*.png /usr/local/share/rofi-bw
cp resources/*.ttf /usr/local/share/rofi-bw

mkdir -p /usr/local/lib/rofi-bw
cp build/lib/plugin.so /usr/local/lib/rofi-bw/plugin.so

mkdir -p /usr/local/bin
cp build/rofi-bw /usr/local/bin/rofi-bw
