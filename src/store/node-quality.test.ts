import { describe, expect, test, vi } from 'vitest'

import type { NodeQualityReport } from '@/services/cmds'
import type {
  ProxyGroupView,
  ProxyNodeView,
  ProxyViewV1,
} from '@/types/proxy-view'

import { NodeQualityManager, qualityNode, qualityScope } from './node-quality'

const capabilities = {
  udp: false,
  xudp: false,
  tfo: false,
  mptcp: false,
  smux: false,
}
const node = (name: string, providerName?: string): ProxyNodeView => ({
  ...capabilities,
  name,
  recordId: name,
  type: 'Shadowsocks',
  alive: true,
  history: [],
  source: providerName
    ? { kind: 'provider', providerName, proxyName: 'same' }
    : { kind: 'core', proxyName: name },
})
const member = (value: ProxyNodeView) => ({
  kind: 'node' as const,
  ref: { kind: 'node' as const, name: value.name, recordId: value.recordId },
  node: value,
})
const group = (
  name: string,
  now: string,
  members: ProxyGroupView['members'],
): ProxyGroupView => ({
  ...capabilities,
  name,
  now,
  members,
  type: 'Selector',
  alive: true,
  history: [],
})
const view = (...nodes: ProxyNodeView[]): ProxyViewV1 => ({
  schemaVersion: 1,
  orderSource: 'runtime',
  providerState: 'ready',
  global: null,
  direct: 'DIRECT',
  groups: [],
  records: Object.fromEntries(nodes.map((value) => [value.recordId, value])),
  standalone: nodes.map((value) => value.recordId),
  providers: [],
})
const report = (generation = 'g1'): NodeQualityReport => ({
  ip: '192.0.2.1',
  fraudScore: 15,
  isResidential: false,
  isBroadcast: null,
  countryCode: 'ZZ',
  asOrganization: 'Example',
  asn: 64500,
  checkedAt: '2026-10-21T00:00:00Z',
  generation,
})
const deferred = <T>() => {
  let resolve!: (value: T) => void
  let reject!: () => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}
const flush = async () => {
  for (let i = 0; i < 12; i += 1) await Promise.resolve()
}

describe('node quality queue', () => {
  test('limits physical requests to two, deduplicates and forces later refreshes', async () => {
    const nodes = [node('a', 'provider-a'), node('b', 'provider-b'), node('c')]
    const requests: ReturnType<typeof deferred<NodeQualityReport>>[] = []
    const check = vi.fn(() => {
      const request = deferred<NodeQualityReport>()
      requests.push(request)
      return request.promise
    })
    const getContext = vi.fn(async () => 'g1')
    const manager = new NodeQualityManager({ getContext, check })
    manager.update(view(...nodes), 'profile', true)
    const first = manager.refresh(nodes.map(member))
    const duplicate = manager.refresh([member(nodes[0])])
    await flush()
    expect(check.mock.calls).toHaveLength(2)
    expect(manager.state('c')?.status).toBe('queued')
    requests[0].resolve(report())
    await flush()
    expect(check.mock.calls).toHaveLength(3)
    requests[1].resolve(report())
    requests[2].resolve(report())
    await Promise.all([first, duplicate])
    expect(check).toHaveBeenNthCalledWith(1, 'a', 'g1')
    expect(check).toHaveBeenNthCalledWith(2, 'b', 'g1')
    const later = manager.refresh([member(nodes[0])])
    await flush()
    expect(check.mock.calls).toHaveLength(4)
    expect(manager.state('a')).toEqual({ report: report(), status: 'checking' })
    requests[3].resolve(report())
    await later
    expect(getContext.mock.calls).toHaveLength(7)
  })

  test('retains the previous report on failure without retrying; first failure is explicit', async () => {
    const value = node('a')
    const check = vi
      .fn()
      .mockResolvedValueOnce(report())
      .mockRejectedValue(new Error('failure'))
    const manager = new NodeQualityManager({
      getContext: async () => 'g1',
      check,
    })
    manager.update(view(value), 'profile', true)
    await manager.refresh([member(value)])
    await manager.refresh([member(value)])
    expect(manager.state('a')).toEqual({ report: report(), status: 'failed' })
    expect(check).toHaveBeenCalledTimes(2)
    manager.update(view(value), 'other-profile', true)
    await manager.refresh([member(value)])
    expect(manager.state('a')).toEqual({ report: undefined, status: 'failed' })
    expect(check).toHaveBeenCalledTimes(3)
  })

  test.each(['profile', 'provider', 'structure', 'disconnect'])(
    'clears queued work and rejects old writes after %s changes',
    async (change) => {
      const nodes = [node('a'), node('b'), node('c')]
      const request = deferred<NodeQualityReport>()
      const check = vi.fn(() => request.promise)
      const manager = new NodeQualityManager({
        getContext: async () => 'g1',
        check,
      })
      const original = view(...nodes)
      manager.update(original, 'profile-1', true)
      const pending = manager.refresh(nodes.map(member))
      await flush()
      const next = structuredClone(original)
      if (change === 'provider')
        next.providers = [
          {
            name: 'p',
            vehicleType: 'HTTP',
            updatedAt: 'new',
            proxyRecordIds: [],
          },
        ]
      if (change === 'structure')
        next.records.a.source = { kind: 'core', proxyName: 'replacement' }
      manager.update(
        next,
        change === 'profile' ? 'profile-2' : 'profile-1',
        change !== 'disconnect',
      )
      request.resolve(report())
      await pending
      await flush()
      expect(manager.state('a')).toBeUndefined()
      expect(manager.state('c')).toBeUndefined()
      expect(check).toHaveBeenCalledTimes(2)
    },
  )

  test('history updates preserve results; a backend generation change discards them', async () => {
    const value = node('a')
    let generation = 'g1'
    const check = vi.fn(async () => report(generation))
    const manager = new NodeQualityManager({
      getContext: async () => generation,
      check,
    })
    const original = view(value)
    manager.update(original, 'profile', true)
    await manager.refresh([member(value)])
    const next = structuredClone(original)
    next.records.a.history.push({ time: 'new', delay: 100 })
    next.records.a.alive = false
    manager.update(next, 'profile', true)
    expect(manager.state('a')?.report).toEqual(report())
    const request = deferred<NodeQualityReport>()
    check.mockImplementationOnce(() => request.promise)
    const pending = manager.refresh([member(value)])
    await flush()
    generation = 'g2'
    request.resolve(report())
    await pending
    expect(manager.state('a')).toBeUndefined()
    await manager.refresh([member(value)])
    expect(manager.state('a')?.report?.generation).toBe('g2')
  })

  test('a disconnected context clears old reports and shows failure without invoking checks', async () => {
    const value = node('a')
    const getContext = vi.fn(async () => 'g1')
    const check = vi.fn(async () => report())
    const manager = new NodeQualityManager({ getContext, check })
    manager.update(view(value), 'profile', true)
    await manager.refresh([member(value)])
    getContext.mockRejectedValue(new Error('not active'))
    await manager.refresh([member(value)])
    expect(manager.state('a')).toEqual({ status: 'failed' })
    expect(check).toHaveBeenCalledTimes(1)
  })
})

test('resolves nested current groups, rejects loops and built-in proxies', () => {
  const value = node('a')
  const snapshot = view(value)
  const inner = group('inner', 'a', [member(value).ref])
  const outer = group('outer', 'inner', [{ kind: 'group', name: 'inner' }])
  snapshot.groups = [outer, inner]
  const selected = {
    kind: 'group' as const,
    ref: { kind: 'group' as const, name: 'outer' },
    group: outer,
  }
  expect(qualityNode(selected, snapshot)).toBe(value)
  inner.now = 'outer'
  inner.members = [{ kind: 'group', name: 'outer' }]
  expect(qualityNode(selected, snapshot)).toBeUndefined()
  for (const name of [
    'DIRECT',
    'REJECT',
    'REJECT-DROP',
    'PASS',
    'COMPATIBLE',
    '',
  ]) {
    const builtin = node(name)
    expect(qualityNode(member(builtin), view(builtin))).toBeUndefined()
  }
  const before = qualityScope(snapshot)
  outer.history.push({ time: 'later', delay: 20 })
  outer.now = 'a'
  expect(qualityScope(snapshot)).toBe(before)
})
