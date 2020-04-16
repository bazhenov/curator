import React, { useState } from 'react'
import { Layout, ExecutionList, TaskSuggest } from '../components'
import { executions } from './fixture'
import { action } from '@storybook/addon-actions';

import "@blueprintjs/core/lib/css/blueprint.css";
import "@blueprintjs/icons/lib/css/blueprint-icons.css";
import { Button } from '@blueprintjs/core';

export default {
  title: "Curator"
}

let agents = [
  {
    application: "app1",
    instance: "single",
    tasks: [
      {
        id: "task1"
      },
      {
        id: "task2",
        description: "Some task description"
      }
    ]
  },
  {
    application: "app2",
    instance: "single",
    tasks: [
      {
        id: "task1",
        description: "Some task description"
      }
    ]
  },
]

export const layout = () => <Layout
  footer={<b>footer</b>}
  header={"header"}
  sidebar={"sidebar"}
  content={"content"}/>

export const executionList = () => <ExecutionList onSelect={action('onSelect')} executions={executions} />

export const taskSuggest = () =>
  <TaskSuggest
    agents={agents}
    isOpen={true}
    forceQuery="ta"
    onSelect={action('onSelect')} />

export const managedTaskSuggest = () => <ManagedTaskSuggest />

function ManagedTaskSuggest() {
  let [isOpen, setOpen] = useState(false);
  return <>
    <Button onClick={() => setOpen(true)}>Open</Button>
    <TaskSuggest
      agents={agents}
      isOpen={isOpen}
      onSelect={() => setOpen(false)} />
  </>
}
    