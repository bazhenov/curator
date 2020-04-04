import React from 'react'
import { Button } from '@blueprintjs/core'

import styles from '../layout.module.css'

export const B: React.SFC<{}> = () => <>
  <p className={styles.foo}>Hello</p>
  <Button intent="success" text="button content" />
</>