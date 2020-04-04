.PHONY: %.curl

URL = http://localhost:8080

agents.curl:
	curl -fs ${URL}/agents | jq .

executions.curl:
	curl -fs ${URL}/executions | jq .

%.curl: %.json
	curl -iXPOST -H "Content-Type: application/json" --data "@$<" "${URL}/task/run"
