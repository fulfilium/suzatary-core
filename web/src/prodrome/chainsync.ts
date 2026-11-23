// Each todo chain the tab reads, held by the core in its worker as a store of objects: restored from the device's log,
// pulled from `GET /api/<chain>/objects` and its own writes posted to `POST /api/<chain>/objects`, by the engines every
// store of objects goes through (`replicasync`, `written`). The page holds none of it and asks.

import type { ChainStore, Core, Standing } from "./core"
import { boxHeadsIn, policyOf, standingIn } from "./core"
import { carried, dropCarried } from "./objects"
import type excluded from "./excluded"
import copies from "./copies"
import replicalog from "./replicalog"
import replicasync from "./replicasync"
import signer from "./signer"
import written from "./written"
import idb from "../app/idb"
import d from "../wire/decoder"
import storereads from "../wire/storereads"

export namespace chainsync {
  export type Chain = { readonly name: string; readonly store: ChainStore; readonly sync: replicasync.T }

  // `Chain`: a reading of the chain moved, and the heads the box last said of it, none before it spoke. `Ours`: the heads
  // the box answered this tab's post with, which its stream then names back. `Sealed`: every write of `chain` this
  // device holds unsent, signatures included, said once an act is kept and before anything of it is posted, so the
  // stream naming one back is known as ours whichever arrives first.
  export type Said =
    | { readonly type: "Chain"; readonly chain: string; readonly box: readonly string[] | null }
    | { readonly type: "Ours"; readonly heads: readonly string[] }
    | { readonly type: "Sealed"; readonly chain: string; readonly names: readonly string[] }

  export type Effects = {
    readonly told: (reader: string, left: excluded.Left) => void
    readonly said: (s: Said) => void
    readonly refused: (why: string) => void
  }

  export type T = {
    // The chains the box serves, the open one among them: each is held, and pulled with the open one's exchange.
    readonly serve: (names: readonly string[]) => void
    // Every chain held, each restored or not yet.
    readonly held: () => readonly Chain[]
    // `name` restored, the acts an earlier build kept of it carried in.
    readonly ready: (name: string) => Promise<Chain>
    // What `name` wrote handed over, then `name` pulled, then every other chain served the same; whether `name`'s pull
    // ended whole.
    readonly exchange: (name: string) => Promise<boolean>
    // `event` sealed onto `name` as `actor`, signed where the box names a root, kept, and handed over behind the answer.
    readonly act: (name: string, event: string, actor: string) => Promise<Chain>
    // What `key` is to `actor` in `name`, read from what stands; none where the box names no root.
    readonly standing: (name: string, actor: string, key: string) => Promise<Standing | null>
    // `name`'s unsent writes `shown` let go and the chain taken again from the box; what the reset could not do. Refused
    // while it holds an unsent write the reader was not shown.
    readonly reset: (name: string, shown: ReadonlySet<string>) => Promise<readonly string[]>
    readonly shown: (on: boolean) => Promise<void>
  }

  // What an act says where it cannot be kept, in the words the page shows.
  const UNLANDED = "this browser holds no chain yet — syncing first; nothing was recorded"
  const NO_KEY = "this browser cannot sign what it writes (it holds no key yet), so the box would refuse it"
  const REVOKED = "this device's key was revoked, so nothing it writes is taken"
  const UNTRUSTED =
    "this device is not trusted to write yet: trust it from Settings → This device with the code from the box, or ask there, then tap its card on a trusted device"

  const Losses = d.json(d.obj<{ losses: string[] }>({ losses: d.arr(d.str) }))
  // The name of every write the chain holds unsent, from its posts.
  const Posts = d.json(d.arr(d.arr(d.obj<{ hash: string }>({ hash: d.str }))))
  const unsentNames = (store: ChainStore): readonly string[] => {
    const posts = Posts(store.unsent())
    return posts.type === "Val" ? posts.v.flat().map(o => o.hash) : []
  }

  // A chain's row as builds before the chains moved into the core kept it: all it held, and the names the box was known
  // to hold. What it held beyond those is what the box has not taken.
  const Row = d.map(
    d.obj<{ objects: { hash: string; literal: string }[]; synced: string[] }>({
      objects: d.arr(d.obj<{ hash: string; literal: string }>({ hash: d.str, literal: d.str })),
      synced: d.arr(d.str),
    }),
    row => {
      const synced = new Set(row.synced)
      return row.objects.filter(o => !synced.has(o.hash)).map(o => o.literal)
    },
  )

  export const of = (core: Core, fx: Effects): T => {
    const chains = new Map<string, Chain>()
    let served: readonly string[] = []

    const writes = written.of<string>(
      core,
      name => {
        const c = of1(name)
        return {
          noun: "act",
          store: c.store,
          exclusive: c.sync.exclusive,
          stepped: step => c.sync.stepped(step),
          post: (literals, since) =>
            storereads.chainPost(name, literals, since).then(receipt => {
              if (receipt.heads.length > 0) fx.said({ type: "Ours", heads: receipt.heads })
              return receipt
            }),
          signs: () => {
            const policy = policyOf(c.store)
            return policy === null ? "Unknown" : policy.root === null ? "Unsigned" : "Signs"
          },
        }
      },
      fx.refused,
    )

    // A chain is made the first time anything names it.
    const of1 = (name: string): Chain => {
      const known = chains.get(name)
      if (known !== undefined) return known
      const store = new core.Chain()
      const sync = replicasync.of(
        {
          name: copies.chain(name),
          held: store,
          // A chain's answer is whole: nothing rests behind it.
          ask: since => storereads.chain(name, since).then(a => ({ ...a, rest: 0, size: 0, weight: 0 })),
          piece: (a, at) => store.piece(JSON.stringify(a), at),
          landed: () => Promise.resolve(),
          form: bytes => replicalog.formOf(core.objects_form(bytes)),
          holds: "OwnWrites",
        },
        {
          told: fx.told,
          bars: () => {},
          // Every move is said, even one that moved no reading: what is unsent and the box's heads are read too.
          moved: () => fx.said({ type: "Chain", chain: name, box: boxHeadsIn(store) }),
          replaced: () => {},
        },
      )
      const made = { name, store, sync }
      chains.set(name, made)
      return made
    }

    const ready = async (name: string): Promise<Chain> => {
      const c = of1(name)
      await c.sync.sync.restored.get()
      await carrying(c)
      return c
    }
    // Once per chain and page: the acts an earlier build kept, written as this tab's own and kept on the device before
    // their row goes. A row that does not read is kept, and named with the one reset that writes over it.
    const carries = new Map<string, Promise<void>>()
    const carryRow = (chain: string): string => `carry/${chain}`
    const carrying = (c: Chain): Promise<void> => {
      const known = carries.get(c.name)
      if (known !== undefined) return known
      const p = carried(
        c.name,
        Row,
        literals => (literals.length === 0 ? Promise.resolve() : c.sync.exclusive(() => c.sync.kept(c.store.carry(JSON.stringify(literals))))),
        why => fx.told(carryRow(c.name), copies.unread(copies.chain(c.name), `the acts an earlier build kept unsent: ${why}`)),
      )
      // A carry that failed is tried again by the next question of the chain.
      carries.set(c.name, p.catch((e: unknown) => {
        carries.delete(c.name)
        throw e
      }))
      return p
    }

    // What another tab let go or wrote is read before anything is posted; nothing is posted before the box named the
    // chain's root, so what a pull names it for goes after the pull.
    const pull = async (name: string): Promise<boolean> => {
      const c = await ready(name)
      await c.sync.caughtUp()
      // The hand-over goes beside the pull, never before it: a post the box is slow to answer delays no pull.
      const handed = writes.handOver(name).catch(() => {})
      const whole = await c.sync.want()
      // What waited for the box to name the chain's root goes once a pull has named it, on the same lane, after the first.
      await handed
      await writes.handOver(name).catch(() => {})
      return whole
    }
    const exchange = async (name: string): Promise<boolean> => {
      const whole = await pull(name)
      for (const other of served) if (other !== name) void pull(other)
      return whole
    }

    const standing = async (name: string, actor: string, key: string): Promise<Standing | null> => {
      const c = await ready(name)
      const root = policyOf(c.store)?.root ?? null
      if (root === null) return null
      const todos = c.store.todos()
      try {
        return standingIn(todos, root, actor, key)
      } finally {
        todos.free()
      }
    }

    // Under a root nothing is sealed that this device cannot sign with a key the chain trusts: the box would refuse it.
    // A key trusted since the last pull reads trusted after one exchange.
    const trusted = async (name: string, actor: string): Promise<void> => {
      if ((policyOf((await ready(name)).store)?.root ?? null) === null) return
      const key = await signer.existing()
      if (key === null) throw new Error(NO_KEY)
      const now = await standing(name, actor, key.public)
      const after = now === null || now === "trusted" ? now : (await exchange(name), await standing(name, actor, key.public))
      if (after === "revoked") throw new Error(REVOKED)
      if (after === "untrusted") throw new Error(UNTRUSTED)
    }

    // Sealed and kept in the chain's turn; once kept the act stands, and a signature that fails is said, never thrown: the
    // next hand-over signs it.
    const act = async (name: string, event: string, actor: string): Promise<Chain> => {
      if (!idb.available()) throw new Error("this browser keeps nothing (no IndexedDB), so an act here would be forgotten — nothing was recorded")
      const c = await ready(name)
      // What another tab did is read first: a reset there is what the act is sealed onto. A chain this device never
      // landed, or one a reset started over, is filled from the box before anything is sealed onto it.
      await c.sync.caughtUp()
      if (!c.sync.sync.landed()) await exchange(name)
      if (!c.sync.sync.landed()) throw new Error(UNLANDED)
      await trusted(name, actor)
      await c.sync.exclusive(async () => {
        // Read again in the act's own turn, so the plan is made over what the device holds now.
        await c.sync.read()
        if (!c.sync.sync.landed()) throw new Error(UNLANDED)
        const frame = c.store.append(event)
        if (frame === undefined) return
        try {
          await c.sync.kept(frame)
        } catch (e: unknown) {
          throw new Error(`this browser could not keep the act (${e instanceof Error ? e.message : String(e)}) — nothing was recorded`)
        }
      })
      await writes.sign(name).catch((e: unknown) => fx.refused(`The act was kept unsigned (${e instanceof Error ? e.message : String(e)}); the next sync signs it.`))
      fx.said({ type: "Sealed", chain: name, names: unsentNames(c.store) })
      void exchange(name)
      return c
    }

    // In the reset's own turn, once the log is read: an act another tab kept and the reader was not shown stops it; what
    // the reader was shown is let go and kept before anything is dropped. A row of an earlier build that did not read is
    // written over.
    const reset = async (name: string, shown: ReadonlySet<string>): Promise<readonly string[]> => {
      const c = await ready(name).catch(() => of1(name))
      const stopped = await c.sync.resetAfter(async () => {
        const read = Losses(c.store.unsent_reading())
        if (read.type === "Error") return [`the core's unsent reading: ${read.e}`]
        const losses = read.v.losses
        const unshown = losses.filter(l => !shown.has(l)).length
        if (unshown > 0) return [`this browser kept ${unshown} more unsent ${unshown === 1 ? "act" : "acts"} since the reset was offered, so nothing was reset: look again at what it loses`]
        if (losses.length > 0) await c.sync.kept(c.store.let_go(JSON.stringify(losses)))
        return []
      })
      if (stopped.length === 0) {
        await dropCarried(name)
        carries.delete(name)
        fx.told(carryRow(name), copies.unread(copies.chain(name), null))
      }
      return stopped
    }

    return {
      serve: names => {
        served = names
        for (const name of names) of1(name)
      },
      held: () => [...chains.values()],
      ready,
      exchange,
      act,
      standing,
      reset,
      shown: on => Promise.all([...chains.values()].map(c => c.sync.shown(on))).then(() => undefined),
    }
  }
}
export default chainsync
