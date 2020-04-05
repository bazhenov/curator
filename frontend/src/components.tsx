import React from 'react'
import { Button, Navbar, Alignment, Intent, ProgressBar, Tag } from '@blueprintjs/core'

import './index.scss'
import { Execution, ExecutionStatus } from './models'
import moment from 'moment'

export const B: React.FC<{}> = () => <>
  <p>Hello</p>
  <Button intent="success" text="button content" />
</>

interface LayoutProps {
  sidebar: React.FC,
  content: React.FC
}

export const Layout: React.FC<LayoutProps> = (props) => <div className="layout">
  <div className="header">
    <Navbar>
      <Navbar.Group align={Alignment.LEFT}>
        <Navbar.Heading>Curator</Navbar.Heading>
        <Navbar.Divider />
        <Button minimal={true} icon="play" text="Run task..." />
      </Navbar.Group>
    </Navbar>
  </div>
  <div className="footer"></div>
  <div className="sidebar">{props.sidebar}</div>
  <div className="content">{props.content}</div>
</div>

interface ExecutionListProps {
  executions: Array<Execution>,
  onSelect?: (_: String) => void,
}

export const ExecutionList: React.FC<ExecutionListProps> = (props) => {
  return <div>
    {props.executions.map(e => <div key={e.id} className="execution">
      <div className="indicator">
        {e.status === ExecutionStatus.COMPLETED &&
          <ProgressBar intent={Intent.SUCCESS} stripes={false}/>}
        {e.status === ExecutionStatus.RUNNING || e.status === ExecutionStatus.INITIATED &&
          <ProgressBar intent={Intent.SUCCESS}/>}
        {e.status === ExecutionStatus.FAILED &&
          <ProgressBar intent={Intent.DANGER} stripes={false}/>}
        {e.status === ExecutionStatus.REJECTED &&
          <ProgressBar intent={Intent.DANGER} stripes={false}/>}
      </div>
      <div className="time">
        {e.finished
          ? <>{moment.duration(moment(e.finished).diff(e.started)).humanize()}</>
          : <>{moment.duration(moment().diff(e.started)).humanize()}</>}
      </div>
      <div className="title">
        {props.onSelect
          ? <a onClick={() => props.onSelect && props?.onSelect(e.id)}><code>{e.task.id}</code></a>
          : <code>{e.task.id}</code>}
      </div>
      <div className="info">
        <Tag minimal={true}>{e.agent.application}</Tag>
        <Tag minimal={true}>{e.agent.instance}</Tag>
      </div>
      <div className="operations">
      </div>
    </div>)}
  </div>
}