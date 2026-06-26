#!/bin/sh
set -eu

[ -d /var/lib/wattdog/samples ] || exit 0

find /var/lib/wattdog/samples \
  -type f \
  -name '*.parquet' \
  -mtime +14 \
  -delete

find /var/lib/wattdog/samples \
  -type d \
  -empty \
  -delete
