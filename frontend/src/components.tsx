import React, { useState } from 'react'
import { Button, Intent, ProgressBar, Tag, MenuItem, Toaster, Position, NonIdealState } from '@blueprintjs/core'
import { ItemRenderer, ItemPredicate, Omnibar } from "@blueprintjs/select"

import './index.scss'
import { Execution, ExecutionStatus, Task, Agent } from './models'
import moment from 'moment'

export const B: React.FC<{}> = () => <>
  <p>Hello</p>
  <Button intent="success" text="button content" />
</>

interface LayoutProps {
  sidebar: React.ReactNode,
  content: React.ReactNode,
  header: React.ReactNode
}

export const Layout: React.FC<LayoutProps> = (props) => <div className="layout">
  <div className="header">{props.header}</div>
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
        {(e.status === ExecutionStatus.RUNNING || e.status === ExecutionStatus.INITIATED) &&
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

interface TaskSuggestProps {
  agents: Array<Agent>,
  onSelect: (a: Agent, t: Task) => void
  isOpen: boolean,
  onClose: () => void,
  forceQuery?: string,
}

export const TaskSuggest: React.FC<TaskSuggestProps> = (props) => {
  type AgentTask = [Agent, Task]

  const TaskSelect = Omnibar.ofType<AgentTask>()
  const predicate : ItemPredicate<AgentTask> = (query, agentTask) => {
    let idMatch = agentTask[1].id.indexOf(query) >= 0;
    if ( idMatch )
      return true
    let descriptionMatch = (agentTask[1].description?.indexOf(query) || -1) >= 0
    return descriptionMatch;
  }
    
  let renderer: ItemRenderer<AgentTask> = (agentTask, {handleClick, modifiers, query}) => {
    if (!modifiers.matchesPredicate) {
      return null
    }
    let [agent, task] = agentTask
    let location = agent.application + "@" + agent.instance;
    return <MenuItem
      active={modifiers.active}
      onClick={handleClick}
      text={
        <>
          {task.id}
          <br />
          <span className="bp3-text-muted">
            {task.description && 
              <Highlight text={task.description} query={query} />}
          </span>
        </>
      }
      labelElement={
        <>
          <Tag minimal={true}>{agent.application}</Tag>&nbsp;
          <Tag minimal={true}>{agent.instance}</Tag>
        </>
      }
      key={task.id + "/" + location}/>
  }

  let tasks = props.agents.flatMap(a => a.tasks.map(t => [a, t] as AgentTask))
    
  //props.forceOpen === undefined ? false : props.forceOpen
  return <TaskSelect
    isOpen={props.isOpen}
    items={tasks}
    query={props.forceQuery}
    itemRenderer={renderer}
    onClose={props.onClose}
    resetOnSelect={true}
    onItemSelect={([agent, task]) => props.onSelect(agent, task)}
    noResults={<MenuItem disabled={true} text="No results." />}
    
    itemPredicate={predicate}/>
}

const Highlight: React.FC<{text: string, query: string}> = (props) => {
  return <>
    {splitForHighlight(props.text, props.query).map(([highlight, text]) => 
      highlight ? <b>{text}</b> : text)}
  </>
}

type HighlightPiece = [boolean, string]

/**
 * Splits the text in chunks providing boolean flag for each chunk based on should it be
 * highlighted or not:
 * 
 * ```
 * splitForHighlight("One of us", "of") => [[false, "One "], [true, "of"], [false, " us"]]
 * ```
 * 
 * @param haystack text to find needle in
 * @param needle the text we are searning for
 */
export function splitForHighlight(haystack: string, needle: string) : Array<HighlightPiece> {
  let result: Array<HighlightPiece> = [];
  let prevOffset = 0;
  let offset = haystack.indexOf(needle);
  while (offset >= 0) {
    if (offset > prevOffset) {
      result.push([false, haystack.substring(offset, prevOffset)])
    }
    result.push([true, needle])
    prevOffset = offset + needle.length
    offset = haystack.indexOf(needle, prevOffset)
  }
  if (prevOffset < haystack.length - 1)
    result.push([false, haystack.substring(prevOffset)])
  return result;
}