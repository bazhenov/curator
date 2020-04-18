import moment from 'moment'

export interface Agent {
  application: string,
  instance: string,
  tasks: Array<Task>
}

export interface AgentRef {
  application: string,
  instance: string
}

export interface Execution {
  id: string,
  agent: AgentRef,
  status: ExecutionStatus,
  output: string,
  task: Task,
  started: moment.Moment,
  finished?: moment.Moment
}

export interface Task {
  id: string,
  description?: string
}

export enum ExecutionStatus {
  RUNNING = "RUNNING",
  INITIATED = "INITIATED",
  REJECTED = "REJECTED",
  COMPLETED = "COMPLETED",
  FAILED = "FAILED"
}