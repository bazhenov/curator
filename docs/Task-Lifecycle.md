# Task-Lifecycle

Each task can be in one of 5 states:

* `INITIATED` - task is initiated by Curator but is not acknowledged by agent yet;
* `REJECTED` - task is rejected by agent. Probably because some precondition are not met;
* `RUNNING` - task is accepted by agent is now running;
* `FAILED` - task was accepted by agent was running but failed to complete;
* `COMPLETED` - task was successfully completed by agent.

Following states are terminal: `REJECTED`, `FAILED`, `COMPLETED`.

[Hello](../reference/REST-API.v1.yaml)