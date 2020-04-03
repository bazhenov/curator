import React, { useEffect, useState } from 'react';
import styles from './App.module.css';

interface AppProps {
  curator: Curator
}

export const App: React.SFC<AppProps> = (props) => {
  const [agents, setAgents] = useState<Array<Agent>>([]);
  let { curator } = props
  useEffect(() => {
    let handler = (a: any) => setAgents(a);
    curator.subscribe(handler);
    return () => props.curator.unsubscribe(handler)
  })

  let taskTemplate = (agent: Agent, task: Task) =>
    <li><a href='#' onClick={() => curator.runTask(task.id, agent)}>{task.id}</a></li>
  let agentTemplate = (agent: Agent) => <li>{agent.application}@{agent.instance}
    <ol>{agent.tasks.map(t => taskTemplate(agent, t))}</ol>
  </li>
  
  return (
    <div className={styles.App}>
      <div>
        <ul>
          {agents.map(agentTemplate)}
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

interface Task {
  id: String
}

export class Curator {

  handlers: Array<any>

  constructor() {
    this.handlers = []
    this.updateLoop()
  }

  updateLoop() {
    fetch("/agents")
      .then(response => response.json())
      .then(r => {
        for (let handler of this.handlers) {
          handler(r)
        }
      })
    setTimeout(() => this.updateLoop(), 1000)
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

  subscribe(handler: AgentChangeListener) {
    this.handlers.push(handler)
  }

  unsubscribe(handler: AgentChangeListener) {
    this.handlers = this.handlers.filter(i => i !== handler);
  }
}

type AgentChangeListener = (_: Array<Agent>) => void