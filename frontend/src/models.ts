export interface Agent {
  readonly name: string
  readonly tasks: Array<Task>
}

export interface Execution {
  readonly id: string
  readonly agent: string
  readonly status: ExecutionStatus
  readonly output: string
  readonly task: Task
  readonly started: string
  readonly finished?: string
  readonly artifact_size?: Number
}

export interface Task {
  readonly name: string
  readonly container_id: string
  readonly description?: string
  readonly labels?: {[index: string]: string}
}

export enum ExecutionStatus {
  RUNNING = "RUNNING",
  INITIATED = "INITIATED",
  REJECTED = "REJECTED",
  COMPLETED = "COMPLETED",
  FAILED = "FAILED",
}

export function hasArtifact(e: Execution): boolean {
  return e.status === ExecutionStatus.COMPLETED ||
    e.status === ExecutionStatus.FAILED
}