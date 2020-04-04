import React, { useEffect, useState } from 'react';
import styles from './App.module.css';

interface AppProps {
  curator: Curator
}

export const App: React.SFC<AppProps> = (props) => {
  const [agents, setAgents] = useState<Array<Agent>>([]);
  const [executions, setExecutions] = useState<Array<Execution>>([]);
  let { curator } = props
  useEffect(curator.onAgentsChange(setAgents))
  useEffect(curator.onExecutionsChange(setExecutions))

  let taskTemplate = (a: Agent, t: Task) =>
    <li><a href='#' onClick={() => curator.runTask(t.id, a)}>{t.id}</a></li>

  let agentTemplate = (agent: Agent) => <li>{agent.application}@{agent.instance}
    <ol>{agent.tasks.map(t => taskTemplate(agent, t))}</ol>
  </li>

  let executionTemplate = (e: Execution) => <li>{e.id}</li>
  
  return (
    <div className={styles.App}>
      <div>
        <h3>Agents</h3>
        <ul>
          {agents.map(agentTemplate)}
        </ul>
      </div>

      <div>
        <h3>Tasks</h3>
        <ul>
          {executions.map(executionTemplate)}
        </ul>
      </div>
    </div>
  );
}

interface Agent {
  application: String,
  instance: String,
  tasks: Array<Task>
}

interface Execution {
  id: String,
  status: String,
  stdout: String
}

interface Task {
  id: String
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
      .then(response => response.json())
      .then(r =>
        this.agentChangeListeners.forEach(listener => listener(r)))
    setTimeout(() => this.updateAgentsLoop(), 1000)
  }

  updateExecutionsLoop() {
    fetch("/executions")
      .then(response => response.json())
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