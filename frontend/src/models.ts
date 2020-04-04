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
  status: String,
  output: String,
  started: moment.Moment,
  finished?: moment.Moment
}

export interface Task {
  id: String
}