import React, { useEffect, useState } from 'react';
import moment from 'moment';
import {Agent, Execution, Task} from './models'
import { ExecutionList } from './components';

interface AppProps {
  curator: Curator
}

export const App: React.SFC<AppProps> = (props) => {
  let { curator } = props

  const [agents, setAgents] = useState<Array<Agent>>([]);
  const [executions, setExecutions] = useState<Array<Execution>>([]);
  const [selectedExecutionId, setSelectedExecutionId] = useState<String | null>(null);
  
  useEffect(curator.onAgentsChange(setAgents))
  useEffect(curator.onExecutionsChange(setExecutions))

  let taskTemplate = (a: Agent, t: Task) =>
<li><a href='#' onClick={() => curator.runTask(t.id, a)}>{t.id}</a></li>

  let agentTemplate = (agent: Agent) => <li>{agent.application}@{agent.instance}
    <ol>{agent.tasks.map(t => taskTemplate(agent, t))}</ol>
  </li>

  let selectedExecution = selectedExecutionId
    ? executions.find(e => e.id === selectedExecutionId)
    : null;
  
  return (
    <div>
      <div>
        <h3>Agents</h3>
        <ul>
          {agents.map(agentTemplate)}
        </ul>
      </div>

      <div>
        <h3>Executions</h3>
        <ul>
          <ExecutionList executions={executions} onSelect={setSelectedExecutionId}/>
        </ul>
      </div>

      {selectedExecution && <ExecutionUI execution={selectedExecution} />}
    </div>
  );
}

const ExecutionUI: React.SFC<{execution: Execution}> = (props) => {
  let { execution } = props
  return <div>
    <p>ExecutionID: {execution.id}</p>
    <p>Status: {execution.status}</p>
    <p>Started: {execution.started.format()}</p>
    <p>Finished: {execution.finished?.format()}</p>
    <p>Agent: <code>{execution.agent.application}@{execution.agent.instance}</code></p>
    <pre>{execution.output}</pre>
  </div>
}

type AgentChangeListener = (_: Array<Agent>) => void
type ExecutionChangeListener = (_: Array<Execution>) => void

export class Curator {

  private agentChangeListeners: Array<AgentChangeListener>
  private executionChangeListeners: Array<ExecutionChangeListener>

  constructor() {
    this.agentChangeListeners = []
    this.executionChangeListeners = []
    this.updateAgentsLoop()
    this.updateExecutionsLoop()
  }

  updateAgentsLoop() {
    fetch("/agents")
      .then(r => r.json())
      .then(r =>
        this.agentChangeListeners.forEach(listener => listener(r)))
    setTimeout(() => this.updateAgentsLoop(), 1000)
  }

  updateExecutionsLoop() {
    fetch("/executions")
      .then(r => r.json())
      .then(r => r.map(processExecutionDates))
      .then(r => r.sort((a: Execution, b: Execution) => a.started.unix() - b.started.unix()))
      .then(r =>
        this.executionChangeListeners.forEach(listener => listener(r)))
    setTimeout(() => this.updateExecutionsLoop(), 1000)
  }

  runTask(task_id: String, agent: Agent) {
    let params = {
      method: "POST",
      body: JSON.stringify({ task_id, agent }),
      headers: {
        'Content-Type': 'application/json'
      }
    }
    fetch("/task/run", params)
  }

  onAgentsChange(listener: AgentChangeListener): () => void {
    this.agentChangeListeners.push(listener)

    return () => {
      this.agentChangeListeners = this.agentChangeListeners.filter(i => i !== listener);
    }
  }

  onExecutionsChange(listener: ExecutionChangeListener): () => void {
    this.executionChangeListeners.push(listener)

    return () => {
      this.executionChangeListeners = this.executionChangeListeners.filter(i => i !== listener);
    }
  }
}

function processExecutionDates(execution: Execution): Execution {
  execution.started = moment(execution.started)
  if ( execution.finished ) {
    execution.finished = moment(execution.finished)
  }
  return execution
}