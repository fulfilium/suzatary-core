// A store's pull, a piece at a time. Each piece the box answers is kept beside what the tab holds, at the cost of that
// piece alone, and the next is asked from the heads they hold together, so a pull cut off keeps every piece that came
// and the next pull asks only for the rest. What the pieces brought lands when the box says nothing is left; a store
// whose box sends each piece closed under parents may land each as it is kept (the lines do, in `keep`).

export namespace pieces {
  // How a pull ended: the box said nothing is left and the pieces landed; a piece moved nothing this tab holds, so asking
  // again would ask the same; or a piece did not come. `taken` counts the pieces kept before it ended.
  export type Ended =
    | { readonly type: "Whole"; readonly taken: number }
    | { readonly type: "Stalled"; readonly taken: number; readonly rest: number }
    | { readonly type: "Cut"; readonly taken: number; readonly e: unknown }

  // `ask` asks from what is held when it is called. `held` is what is held, landed and not, each part the same value
  // while nothing moved it; it is read before a piece is asked for and after it is kept, so a reset that lands while a
  // piece is on its way is a move and the next piece is asked of it.
  export type Effects<A extends { readonly rest: number }> = {
    readonly ask: () => Promise<A>
    readonly keep: (piece: A) => Promise<void>
    readonly land: (last: A) => Promise<void>
    readonly held: () => readonly unknown[]
  }

  const same = (a: readonly unknown[], b: readonly unknown[]): boolean => a.length === b.length && a.every((x, i) => x === b[i])

  export const pull = async <A extends { readonly rest: number }>(fx: Effects<A>): Promise<Ended> => {
    for (let taken = 0; ; taken += 1) {
      const was = fx.held()
      let piece: A
      try {
        piece = await fx.ask()
      } catch (e: unknown) {
        return { type: "Cut", taken, e }
      }
      await fx.keep(piece)
      if (piece.rest === 0) {
        await fx.land(piece)
        return { type: "Whole", taken: taken + 1 }
      }
      if (same(fx.held(), was)) return { type: "Stalled", taken: taken + 1, rest: piece.rest }
    }
  }
}
export default pieces
