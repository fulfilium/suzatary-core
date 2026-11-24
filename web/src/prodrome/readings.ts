// What this device's readings of the replicas it holds left out, by the replica read.

import excluded from "./excluded"
import sources from "../algebra/source"

export namespace readings {
  const source = sources.create<excluded.Seen, excluded.Event>(excluded.UNSEEN, excluded.step)

  // A reader names the replica it read, so a later reading of it replaces what the last one left out.
  export const told = (reader: string, left: excluded.Left): void => source.dispatch({ type: "Read", reader, left })

  export const left = (): excluded.Left => excluded.all(source.state())

  export const watch = (listener: () => void): (() => void) => source.subscribe(() => listener())
}
export default readings
