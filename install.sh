#!/bin/sh

set -eu

cd "$(dirname $0)"

mkdir -p /usr/local/bin
cp build/rofi-bw /usr/local/bin/rofi-bw

mkdir -p /usr/local/lib/rofi-bw
cp build/lib/plugin.so /usr/local/lib/rofi-bw/plugin.so
