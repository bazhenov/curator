import React from "react"
import { ExecutionStatus, Execution } from "./models"
import { Intent, Tag, Spinner } from "@blueprintjs/core"
import moment from "moment"

interface ExecutionListProps {
  executions: Array<Execution>,
  onSelect?: (_: String) => void,
}

export const CardExecutionList: React.FC<ExecutionListProps> = (props) => {
  return <div className="execution-brief-container">
    {props.executions.map(e => <div key={e.id} className="execution"
      onClick={() => props.onSelect?.(e.id)}>
      <div className="indicator">
        {e.status === ExecutionStatus.COMPLETED &&
          <Spinner intent={Intent.SUCCESS} value={1.0} size={Spinner.SIZE_SMALL}/>}
        {(e.status === ExecutionStatus.RUNNING || e.status === ExecutionStatus.INITIATED) &&
          <Spinner intent={Intent.WARNING} size={Spinner.SIZE_SMALL}/>}
        {(e.status === ExecutionStatus.FAILED || e.status === ExecutionStatus.REJECTED) &&
          <Spinner intent={Intent.DANGER} value={1.0} size={Spinner.SIZE_SMALL}/>}
      </div>
      <div className="info">
        {e.finished
          ? <>{moment.duration(moment(e.finished).diff(e.started)).humanize()}</>
          : <>{moment.duration(moment().diff(e.started)).humanize()}</>}
      </div>
      <div className="title">
        <ol>
          {splitTaskId(e.task.id).map(part => <li>{part}</li>)}
        </ol>
      </div>
      <div className="tags">
        <Tag>{e.agent}</Tag>
      </div>
    </div>)}
  </div>
}

function splitTaskId(taskId: string) : Array<string> {
  return taskId.split("/")
}