// Places a host's keyed elements by an arrangement of tiles (the JSON of `tiles::Arranged`): the style that stands an
// element where its tile is, and the edits that put the drawn elements in a container and the hidden ones out of it.
// It names no page, route or store of any application, and it touches no DOM: the host applies the edits.

export namespace placing {
  export type Rect = { readonly x: number; readonly y: number; readonly w: number; readonly h: number }
  // The room the tiles stand in, in the arrangement's unit.
  export type Room = { readonly w: number; readonly h: number }
  // A tile of the arrangement: the leaf's key, its rectangle in the room, and where its page stands inside the rectangle.
  export type Tile = { readonly id: number; readonly rect: Rect; readonly fill: Rect }

  const pct = (n: number, of: number): string => `${((100 * n) / Math.max(1, of)).toFixed(3)}%`

  // The rectangle as shares of the room, so a room of any size is filled by the same text.
  export const rectStyle = (room: Room, r: Rect): string =>
    `left:${pct(r.x, room.w)};top:${pct(r.y, room.h)};width:${pct(r.w, room.w)};height:${pct(r.h, room.h)}`

  // Where the page stands inside its tile as padding in rem (top, right, bottom, left), `rem` being the pixels to one: none where the
  // page fills it.
  export const inset = (t: Tile, rem: number): string => {
    const [top, right, bottom, left] = [t.fill.y - t.rect.y, t.rect.x + t.rect.w - t.fill.x - t.fill.w, t.rect.y + t.rect.h - t.fill.y - t.fill.h, t.fill.x - t.rect.x]
    return top === 0 && right === 0 && bottom === 0 && left === 0 ? "" : `padding:${top / rem}rem ${right / rem}rem ${bottom / rem}rem ${left / rem}rem`
  }

  // An element goes in the container before `before`, or last where that is null; or goes out of it.
  export type Edit<E extends object> = { readonly type: "Insert"; readonly e: E; readonly before: E | null } | { readonly type: "Remove"; readonly e: E }

  export type Placed<E extends object> = {
    // The elements of the tiles drawn, in the arrangement's order.
    readonly drawn: readonly E[]
    // The elements the host keeps that no tile draws: alive, and out of the container.
    readonly hidden: readonly E[]
    readonly edits: readonly Edit<E>[]
  }

  // An element is an object, which the sets here tell apart by identity.
  // `attached` is the container's children that are the host's keyed elements, in order; `end` is what a newcomer goes
  // before where no element that stays follows it (a layer of the host's, or null for the container's end). An element
  // drawn before and still drawn is never moved, so it keeps what its page holds; one drawn newly goes right before the
  // next that stays, which puts the container in the arrangement's order wherever those that stay already stand in it.
  export const place = <E extends object>(tiles: readonly Tile[], elements: ReadonlyMap<number, E>, attached: readonly E[], end: E | null): Placed<E> => {
    const drawn = tiles.flatMap(t => elements.get(t.id) ?? [])
    const shown = new Set(drawn)
    const there = new Set(attached)
    const stays = new Set(drawn.filter(e => there.has(e)))
    const hidden = [...elements.values()].filter(e => !shown.has(e))
    const out: Edit<E>[] = attached.filter(e => !shown.has(e)).map(e => ({ type: "Remove", e }))
    drawn.forEach((e, i) => {
      if (!stays.has(e)) out.push({ type: "Insert", e, before: drawn.slice(i + 1).find(u => stays.has(u)) ?? end })
    })
    return { drawn, hidden, edits: out }
  }
}
export default placing
