import React from 'react'
import { ExecutionList, ExecutionUI } from '../components'
import { executions } from './fixture'
import { action } from '@storybook/addon-actions';

export default {
  title: "Executions"
}

export const executionList = () => <ExecutionList onSelect={action('onSelect')} executions={executions} />

export const runningExecution = ()  => <ExecutionUI execution={executions[0]} />
export const finishedExecution = ()  => <ExecutionUI execution={executions[1]} />
export const failedExecution = ()  => <ExecutionUI execution={executions[2]} />