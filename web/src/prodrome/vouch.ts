// A trusted device vouching for a new one (SPEC law 42): its key's signature of the registration the card asks for,
// read by the worker off its replica of the first chain, which is where the box keeps it.

import app from "../app/app"
import { ask } from "./sync"
import signer from "./signer"

export namespace vouch {
  export type Endorsement = { readonly key: string; readonly signature: string }

  export const endorsement = async (actor: string, key: string): Promise<Endorsement> => {
    const first = app.now().served[0]
    if (first === undefined) throw new Error("this device holds no copy of the first chain yet, so it cannot vouch")
    const held = await signer.held()
    if (held.type !== "Held") throw new Error(`this browser cannot sign (${held.e}), so it cannot vouch`)
    const added = await ask({ type: "KeyAdded", chain: first, actor, key }, "Page")
    return { key: held.key.public, signature: await signer.sign(held.key, added) }
  }
}
export default vouch
