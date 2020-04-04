import React from 'react'
import { Button, Navbar, Alignment, Icon } from '@blueprintjs/core'

import styles from './app.module.css'
import { Execution } from './models'

export const B: React.FC<{}> = () => <>
  <p className={styles.foo}>Hello</p>
  <Button intent="success" text="button content" />
</>

interface LayoutProps {
  sidebar: React.FC,
  content: React.FC
}

export const Layout: React.FC<LayoutProps> = (props) => <div className={styles.layout}>
  <div className={styles.header}>
    <Navbar>
      <Navbar.Group align={Alignment.LEFT}>
        <Navbar.Heading>Curator</Navbar.Heading>
        <Navbar.Divider />
        <Button className="bp3-minimal" icon="play" text="Run task..." />
      </Navbar.Group>
    </Navbar>
  </div>
  <div className={styles.footer}></div>
  <div className={styles.sidebar}>{props.sidebar}</div>
  <div className={styles.content}>{props.content}</div>
</div>

export const ExecutionList: React.FC<{ executions: Array<Execution> }> = (props) => {
  return <div>
    {props.executions.map(e => <div className={styles.execution}>
      <Icon icon="record" color="green" />
      <code>{e.id}</code>
    </div>)}
  </div>
}