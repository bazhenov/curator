.PHONY: %.curl

URL = http://localhost:8080

agents.curl:
	curl -fs ${URL}/backend/agents | jq .

executions.curl:
	curl -fs ${URL}/backend/executions | jq .

run-%.curl: run-%.json
	curl -iXPOST -H "Content-Type: application/json" --data "@$<" "${URL}/backend/task/run"

new-agent.curl:
	curl -i -XPOST -H "Content-Type: application/json"  --data "@$(basename $@).json" "${URL}/backend/events"