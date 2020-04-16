#!/usr/bin/env bash
echo '{"id": "foo", "command": "date"}'
echo '{"id": "uptime", "command": "uptime", "description": "Run uptime on a given machine"}'
echo '{"id": "uname", "command": "uname", "args": ["-a"]}'
echo '{"id": "ls", "command": "ls", "args": ["-l", "/"]}'
pgrep zsh | jq -cM '{id: ("vmmap." + (.|tostring)), command: "vmmap", args: [.|tostring]}'