import { useState } from 'react'
import { Layout, ExecutionList, TaskSuggest, ExecutionUI, TaskList } from '../components'
import { executions, agents } from './fixture'
import { action } from '@storybook/addon-actions';

import "@blueprintjs/core/lib/css/blueprint.css";
import "@blueprintjs/icons/lib/css/blueprint-icons.css";
import { Button } from '@blueprintjs/core';

export default {
  title: "Curator"
}

let shortAgents = [
  {
    name: "app1",
    tasks: [
      {
        name: "jstack",
        description: "Running jstack on given JVM",
        labels: { "platform": "java", "app": "my-app-deployment" }
      },
      {
        name: "task2"
      }
    ]
  },
  {
    name: "app2",
    tasks: [
      {
        name: "task1",
        description: "Some task description",
        labels: []
      }
    ]
  },
]

export const layout = () => <Layout
  footer={<b>footer</b>}
  header={"header"}
  sidebar={"sidebar"}
  content={"content"} />

export const executionList = () => <ExecutionList onSelect={action('onSelect')} executions={executions} />

export const Execution = () => <ExecutionUI execution={executions[1]} />

export const taskSuggest = () =>
  <TaskSuggest
    agents={shortAgents}
    isOpen={true}
    forceQuery="ta"
    onSelect={action('onSelect')} />

export const taskSuggestSearchingByTag = () =>
  <TaskSuggest
    agents={shortAgents}
    isOpen={true}
    forceQuery="ta my-app"
    onSelect={action('onSelect')} />

export const managedTaskSuggest = () => <ManagedTaskSuggest />

export const taskList = () => <TaskList agents={agents} onClick={action('click')} />

function ManagedTaskSuggest() {
  let [isOpen, setOpen] = useState(false);
  return <>
    <Button onClick={() => setOpen(true)}>Open</Button>
    <TaskSuggest
      agents={shortAgents}
      isOpen={isOpen}
      onSelect={() => setOpen(false)} />
  </>
}