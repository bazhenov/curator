import React from 'react'
import { Button, Intent, ProgressBar, Tag, MenuItem, Spinner } from '@blueprintjs/core'
import { ItemRenderer, ItemPredicate, Omnibar } from "@blueprintjs/select"
import numeral from "numeral"

import './index.scss'
import { Execution, ExecutionStatus, Task, Agent, hasArtifact } from './models'
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
  executions: Execution[],
  onSelect?: (_: string) => void,
}

export const ExecutionList: React.FC<ExecutionListProps> = (props) => {
  return <div>
    {props.executions.map(e => <ExecutionItem key={e.id} execution={e} onSelect={props.onSelect} />)}
  </div>
}

type ExecutionProps = {
  execution: Execution,
  onSelect?: (_: string) => void,
}

const ExecutionItem = (props: ExecutionProps) => {
  let e = props.execution;
  let [intent, stripes] = intentFromStatus(e.status);

  return <div key={e.id} className="execution">
    <div className="indicator">
      <ProgressBar intent={intent} stripes={stripes} />
    </div>
    <div className="time">
      {e.finished
        ? <>{moment.duration(moment(e.finished).diff(e.started)).humanize()}</>
        : <>{moment.duration(moment().diff(e.started)).humanize()}</>}
    </div>
    <div className="title">
      {props.onSelect
        ? <a onClick={() => props.onSelect && props.onSelect(e.id)}><code>{e.task.name}</code></a>
        : <code>{e.task.name}</code>}
    </div>
    <div className="info">
      <Tag>{e.agent}</Tag>
      <TagList labels={e.task.labels} />
    </div>
    <div className="operations">
    </div>
  </div>
}

type IntentAndStripes = [Intent, boolean];
function intentFromStatus(status: ExecutionStatus): IntentAndStripes {
  switch (status) {
    case ExecutionStatus.RUNNING:
      return [Intent.SUCCESS, true]
    case ExecutionStatus.INITIATED:
      return [Intent.SUCCESS, true]

    case ExecutionStatus.COMPLETED:
      return [Intent.SUCCESS, false]

    case ExecutionStatus.FAILED:
      return [Intent.DANGER, false]
    case ExecutionStatus.REJECTED:
      return [Intent.DANGER, false]
  }
}

export const TagList: React.FC<{ labels?: { [index: string]: string } }> = (props) => {
  const alwaysDisplayLabels = ["io.kubernetes.pod.name"]
  // making safe copy
  let labels = { ...props.labels || {} }

  const importantLabelsHtml = alwaysDisplayLabels.map(name => [name, labels[name]])
    .filter(([_, value]) => value !== undefined)
    .map(([name, value]) => <Tag key={name} minimal={true}>{value}</Tag>)

  for (let name of alwaysDisplayLabels) {
    delete labels[name]
  }

  const otherLabelsHtml = Object.entries(labels)
    .map(([k, v]) => <Tag key={k} minimal={true}>
      {v.length > 12 ? <abbr title={v}>{v.substring(0, 12) + "…"}</abbr> : v}
    </Tag>)

  return <>{importantLabelsHtml} {otherLabelsHtml}</>
}

export const ExecutionUI: React.FC<{ execution: Execution }> = (props) => {
  let { execution } = props
  return <div className="execution-full">
    <p>ExecutionID: {execution.id}</p>
    <p>Status: {execution.status}</p>
    <p>Started: {moment(execution.started).format("LLLL")}</p>
    {execution.finished && <p>Finished: {moment(execution.finished).format("LLLL")}</p>}
    <p>Agent: <code>{execution.agent}</code></p>
    {hasArtifact(execution) &&
      <p>
        {execution.artifact_size
          ? <a href={"/backend/artifacts/" + execution.id + ".tar.gz"} role="button"
            className="bp3-button bp3-icon-database bp3-minimal">
            Artifacts ({numeral(execution.artifact_size).format("0 ib")})</a>
          : <a href="#artifact-is-in-progress" role="button"
            className="bp3-button bp3-icon-database bp3-minimal bp3-disabled">
            Artifacts&nbsp;<Spinner size={Spinner.SIZE_SMALL} /></a>}

      </p>}
    <pre>{execution.output}</pre>
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
  const predicate: ItemPredicate<AgentTask> = (query, [, task]) => {
    for (let part of query.split(" ")) {
      if (part === "")
        continue;

      let nameMatch = task.name.indexOf(part) >= 0;
      if (nameMatch)
        continue

      let descriptionMatch = (task.description?.indexOf(part) || -1) >= 0
      if (descriptionMatch)
        continue

      let tagsMatch = Object.entries(task.labels || {}).some(([k, v]) => v.indexOf(part) >= 0)
      if (tagsMatch)
        continue

      return false
    }
    return true
  }

  let renderer: ItemRenderer<AgentTask> = (agentTask, { handleClick, modifiers, query }) => {
    if (!modifiers.matchesPredicate) {
      return null
    }
    let [agent, task] = agentTask
    let location = agent.name;
    return <MenuItem
      active={modifiers.active}
      onClick={handleClick}
      text={
        <>
          {task.name}
          {task.description &&
            <span className="bp3-text-muted">
              <br />
              <Highlight text={task.description} query={query} />
            </span>}
          {task.labels &&
            <span className="task-tags">
              <br />
              <TagList labels={task.labels} />
            </span>}
        </>
      }
      labelElement={<Tag>{agent.name}</Tag>}
      key={task.name + "/" + location} />
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

    itemPredicate={predicate} />
}

const Highlight: React.FC<{ text: string, query: string }> = (props) => {
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
export function splitForHighlight(haystack: string, needle: string): Array<HighlightPiece> {
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