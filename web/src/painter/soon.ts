// A task of its own for `f`, after every task already queued: a message to this tab's own channel, which a hidden tab
// does not hold back as it holds back a timer.

export namespace soon {
  const queued: (() => void)[] = []
  const channel = new MessageChannel()
  channel.port1.onmessage = () => queued.shift()?.()
  export const run = (f: () => void): void => (queued.push(f), channel.port2.postMessage(null))
}
export default soon
