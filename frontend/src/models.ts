import moment from 'moment'

export interface Agent {
  application: String,
  instance: String,
  tasks: Array<Task>
}

export interface AgentRef {
  application: String,
  instance: String
}

export interface Execution {
  id: String,
  agent: AgentRef,
  status: ExecutionStatus,
  output: String,
  task: Task,
  started: moment.Moment,
  finished?: moment.Moment
}

export interface Task {
  id: String
}

export enum ExecutionStatus {
  RUNNING = "RUNNING",
  INITIATED = "INITIATED",
  REJECTED = "REJECTED",
  COMPLETED = "COMPLETED",
  FAILED = "FAILED"
}