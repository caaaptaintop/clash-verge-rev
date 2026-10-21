import {
  checkNodeQuality,
  getNodeQualityContext,
  type NodeQualityReport,
} from '@/services/cmds'
import {
  findCurrentGroupMember,
  resolveMember,
  type ProxyNodeView,
  type ProxyViewV1,
  type ResolvedProxyMember,
} from '@/types/proxy-view'

export interface NodeQualityState {
  report?: NodeQualityReport
  status: 'queued' | 'checking' | 'ready' | 'failed'
}

const excluded = new Set([
  'DIRECT',
  'REJECT',
  'REJECT-DROP',
  'REJECTDROP',
  'PASS',
  'COMPATIBLE',
])

export function qualityNode(
  member: ResolvedProxyMember,
  view: ProxyViewV1,
  visited = new Set<string>(),
): ProxyNodeView | undefined {
  if (!member.ref.name || excluded.has(member.ref.name.toUpperCase())) return
  if (member.kind === 'node') {
    if (excluded.has(member.node.type.toUpperCase())) return
    const current = view.records[member.node.recordId]
    return current &&
      current.name === member.node.name &&
      JSON.stringify(current.source) === JSON.stringify(member.node.source)
      ? current
      : undefined
  }
  if (member.kind !== 'group' || visited.has(member.ref.name)) return
  visited.add(member.ref.name)
  const group =
    view.global?.name === member.ref.name
      ? view.global
      : view.groups.find(({ name }) => name === member.ref.name)
  const current = group && findCurrentGroupMember(view, group)
  return current && qualityNode(current.member, view, visited)
}

export function qualityScope(view: ProxyViewV1): string {
  const structure = (group: ProxyViewV1['global']) =>
    group && [group.name, group.type, group.members]
  return JSON.stringify([
    view.schemaVersion,
    view.orderSource,
    view.direct,
    view.providerState,
    structure(view.global),
    view.groups.map(structure),
    Object.values(view.records).map((node) => [
      node.recordId,
      node.id,
      node.name,
      node.type,
      node.source,
    ]),
    view.standalone,
    view.providers.map((provider) => [
      provider.name,
      provider.vehicleType,
      provider.updatedAt,
      provider.proxyRecordIds,
    ]),
  ])
}

interface Task {
  id: string
  epoch: number
  generation: string
  done: Promise<void>
  resolve: () => void
}

export class NodeQualityManager {
  private view?: ProxyViewV1
  private scope = ''
  private generation?: string
  private epoch = 0
  private active = 0
  private queue: Task[] = []
  private pending = new Map<string, Task>()
  private states = new Map<string, NodeQualityState>()
  private listeners = new Set<() => void>()

  constructor(
    private transport = {
      getContext: getNodeQualityContext,
      check: checkNodeQuality,
    },
  ) {}

  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  state = (id: string) => this.states.get(id)

  node = (member: ResolvedProxyMember) =>
    this.view && qualityNode(member, this.view)

  snapshot = (member: ResolvedProxyMember) => {
    const node = this.node(member)
    return node && (this.states.get(node.recordId) ?? node)
  }

  private notify() {
    this.listeners.forEach((listener) => listener())
  }

  private clear(keep: Task[] = []) {
    this.epoch += 1
    this.generation = undefined
    this.queue = []
    this.pending.forEach((task) => {
      if (!keep.includes(task)) task.resolve()
    })
    this.pending.clear()
    this.states.clear()
    this.notify()
  }

  update(view: ProxyViewV1 | undefined, profile: string, available: boolean) {
    const scope = available && view ? profile + qualityScope(view) : ''
    this.view = available ? view : undefined
    if (scope !== this.scope) {
      this.scope = scope
      this.clear()
    }
  }

  refreshGroup = (name: string) => {
    const group =
      this.view?.global?.name === name
        ? this.view.global
        : this.view?.groups.find((group) => group.name === name)
    return this.refresh(
      this.view && group
        ? group.members.map((ref) => resolveMember(this.view!, ref))
        : [],
    )
  }

  refreshAll = () =>
    this.refresh(
      Object.values(this.view?.records ?? {}).map((node) => ({
        kind: 'node',
        ref: { kind: 'node', name: node.name, recordId: node.recordId },
        node,
      })),
    )

  async refresh(members: readonly ResolvedProxyMember[]): Promise<void> {
    if (!this.view) return
    const epoch = this.epoch
    const ids = new Set(
      members.flatMap((member) => {
        const node = this.node(member)
        return node ? [node.recordId] : []
      }),
    )
    const tasks: Task[] = []
    const waiting: Promise<void>[] = []
    for (const id of ids) {
      const existing = this.pending.get(id)
      if (existing) {
        waiting.push(existing.done)
        continue
      }
      let resolve!: () => void
      const done = new Promise<void>((finish) => {
        resolve = finish
      })
      const task = { id, epoch, generation: '', done, resolve }
      this.pending.set(id, task)
      tasks.push(task)
      waiting.push(done)
      this.states.set(id, {
        report: this.states.get(id)?.report,
        status: 'queued',
      })
    }
    this.notify()
    try {
      const generation = await this.transport.getContext()
      if (epoch !== this.epoch) return
      if (this.generation && this.generation !== generation) {
        this.clear(tasks)
        tasks.forEach((task) => {
          task.epoch = this.epoch
          this.pending.set(task.id, task)
          this.states.set(task.id, { status: 'queued' })
        })
      }
      this.generation = generation
      tasks.forEach((task) => {
        task.generation = generation
      })
      this.queue.push(...tasks)
      this.notify()
      this.pump()
    } catch {
      if (epoch !== this.epoch) return
      this.clear()
      ids.forEach((id) => this.states.set(id, { status: 'failed' }))
      this.notify()
    }
    await Promise.all(waiting)
  }

  private pump() {
    while (this.active < 2 && this.queue.length) {
      const task = this.queue.shift()!
      this.active += 1
      this.states.set(task.id, {
        report: this.states.get(task.id)?.report,
        status: 'checking',
      })
      this.notify()
      void this.run(task)
    }
  }

  private async run(task: Task) {
    try {
      const report = await this.transport.check(task.id, task.generation)
      const current = await this.transport.getContext()
      if (task.epoch !== this.epoch) return
      if (current !== task.generation || report.generation !== current) {
        this.clear()
        return
      }
      this.states.set(task.id, { report, status: 'ready' })
      this.notify()
    } catch {
      if (task.epoch === this.epoch) {
        try {
          const current = await this.transport.getContext()
          if (task.epoch !== this.epoch) return
          if (current !== task.generation) {
            this.clear()
            return
          }
          this.states.set(task.id, {
            report: this.states.get(task.id)?.report,
            status: 'failed',
          })
          this.notify()
        } catch {
          if (task.epoch === this.epoch) {
            this.clear()
            this.states.set(task.id, { status: 'failed' })
            this.notify()
          }
        }
      }
    } finally {
      if (this.pending.get(task.id) === task) this.pending.delete(task.id)
      task.resolve()
      this.active -= 1
      this.pump()
    }
  }
}

export const nodeQualityManager = new NodeQualityManager()
