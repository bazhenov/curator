#!/usr/bin/env sh

set -e

UNITS=$(systemctl list-units --type=service | grep -oP '^[^ ]+.service')
UNITS=$(echo "$UNITS" | awk '{ print "\"" $0 "\""}')
echo "$UNITS" | jq -cM '{
  id: ("systemctl.status." + .),
  command: "systemctl",
  args: ["status", .]
}'