import React, { useEffect, useState } from 'react';
import {Agent, Execution, Task} from './models'
import { ExecutionList, Layout, TaskSuggest, ExecutionUI } from './components';
import { Navbar, Alignment, Button } from '@blueprintjs/core';

interface AppProps {
  curator: Curator
}

export const App: React.SFC<AppProps> = (props) => {
  let { curator } = props

  const [agents, setAgents] = useState<Array<Agent>>([]);
  const [executions, setExecutions] = useState<Array<Execution>>([]);
  const [selectedExecutionId, setSelectedExecutionId] = useState<String | null>(null);
  
  useEffect(() => curator.onAgentsChange(setAgents), [curator, setAgents])
  useEffect(() => curator.onExecutionsChange(setExecutions), [curator, setExecutions])
  let [isOmnibarOpen, setOmnibarOpen] = useState(false);

  let taskTemplate = (a: Agent, t: Task) =>
    <li key={t.name}><a href={'#key-' + t.name} onClick={() => curator.runTask(a, t)}>{t.name}</a></li>

  let agentTemplate = (agent: Agent) => <li key={agent.name}>
    {agent.name}
    <ol>{agent.tasks.map(t => taskTemplate(agent, t))}</ol>
  </li>

  let selectedExecution = selectedExecutionId
    ? executions.find(e => e.id === selectedExecutionId)
    : null;

  let agentsUi = <div>
    <h2>Agents</h2>
    <ul>
      {agents.map(agentTemplate)}
    </ul>
  </div>

  let executionUi = <div>
    <h2>Executions</h2>
    <ExecutionList executions={executions} onSelect={setSelectedExecutionId}/>
    {selectedExecution && <ExecutionUI execution={selectedExecution} />}
  </div>

  let header = <Navbar>
    <Navbar.Group align={Alignment.LEFT}>
      <Navbar.Heading>Curator</Navbar.Heading>
      <Navbar.Divider />
      <TaskSuggest
        isOpen={isOmnibarOpen}
        agents={agents}
        onClose={() => setOmnibarOpen(false)}
        onSelect={(a, t) => {
          curator.runTask(a, t)
          setOmnibarOpen(false)
        }} />
      <Button minimal={true} icon="play" text="Run task..." onClick={() => setOmnibarOpen(true)} />
    </Navbar.Group>
  </Navbar>
  
  return <Layout header={header} sidebar={agentsUi} content={executionUi} />
}

type AgentChangeListener = (_: Array<Agent>) => void
type ExecutionChangeListener = (_: Array<Execution>) => void

export class Curator {

  private agentChangeListeners: Array<AgentChangeListener>
  private executionChangeListeners: Array<ExecutionChangeListener>
  private agents: Agent[]
  private executions: Execution[]

  constructor() {
    this.agentChangeListeners = []
    this.executionChangeListeners = []
    this.agents = []
    this.executions = []
    this.updateAgentsLoop()
    this.updateExecutionsLoop()
  }

  updateAgentsLoop() {
    fetch("/backend/agents")
      .then(r => r.json())
      .then(r => {
        // Replace with Conditional GET on backend side
        if (JSON.stringify(this.agents) !== JSON.stringify(r)) {
          this.agents = r;
          this.agentChangeListeners.forEach(listener => listener(r))
        }
      })
    setTimeout(() => this.updateAgentsLoop(), 1000)
  }

  updateExecutionsLoop() {
    fetch("/backend/executions")
      .then(r => r.json())
      .then(r => r.sort((a: Execution, b: Execution) => a.started < b.started))
      .then(r => {
        // Replace with Conditional GET on backend side
        if (JSON.stringify(this.executions) !== JSON.stringify(r)) {
          this.executions = r
          this.executionChangeListeners.forEach(listener => listener(r))
        }
      })
    setTimeout(() => this.updateExecutionsLoop(), 1000)
  }

  runTask(agent: Agent, task: Task) {
    let params = {
      method: "POST",
      body: JSON.stringify({ task_id: task.name, agent: agent.name }),
      headers: {
        'Content-Type': 'application/json'
      }
    }
    fetch("/backend/task/run", params)
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