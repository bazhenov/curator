export interface Agent {
  name: string,
  tasks: Array<Task>
}

export interface Execution {
  id: string,
  agent: string,
  status: ExecutionStatus,
  output: string,
  task: Task,
  started: string,
  finished?: string
}

export interface Task {
  id: string,
  description?: string
  tags?: Array<string>
}

export enum ExecutionStatus {
  RUNNING = "RUNNING",
  INITIATED = "INITIATED",
  REJECTED = "REJECTED",
  COMPLETED = "COMPLETED",
  FAILED = "FAILED"
}