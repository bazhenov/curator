import React from 'react'
import { Layout, ExecutionList } from '../components'
import { executions } from './fixture'

import "@blueprintjs/core/lib/css/blueprint.css";
import "@blueprintjs/icons/lib/css/blueprint-icons.css";

export default {
  title: "Curator"
}

export const layout = () => <Layout
  footer={<b>footer</b>}
  sidebar={"sidebar"}
  content={"content"}/>

export const executionList = () => <ExecutionList executions={executions} />