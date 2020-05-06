#!/usr/bin/env sh

set -e

pgrep curator | jq -cM '{
  id: ("lsof." + (. | tostring)),
  command: "lsof",
  args: ["-p", . | tostring]
}'