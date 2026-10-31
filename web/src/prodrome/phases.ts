// The core worker's phases on its first load, as marks in its timeline, which the browser scenes read to time a load
// (`bigstore.test.ts`). A timeline keeps every mark until it is cleared, so each name is marked once: a tab open for days
// holds the same few marks.

export namespace phases {
  const marked = new Set<string>()

  export const mark = (name: string): void => {
    if (marked.has(name)) return
    marked.add(name)
    performance.mark(name)
  }
}
export default phases
