#!/usr/bin/env sh

virsh list --name | head -n -1 | awk '{ print "\"" $0 "\""}' | \
  jq -cM '{
    id: ("Stop '\''" + . + "'\'' domain"),
    "command": "virsh",
    args: ["destroy", .]
  }'

virsh list --inactive --name  | head -n -1 | awk '{ print "\"" $0 "\""}' | \
  jq -cM '{
    id: ("Start '\''" + . + "'\'' domain"),
    "command": "virsh",
    args: ["start", .]
  }'