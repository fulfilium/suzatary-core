// The marks a page leaves on the performance timeline when it has drawn something whole, for whoever measures it.

export namespace marks {
  // A conversation's window drawn whole and its place laid out: the end of a redraw. Only the latest is kept.
  export const PLACED = "transcript:placed"
}
export default marks
