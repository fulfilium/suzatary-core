// The core worker's first-load phases, as timeline marks the browser scenes read to time a load (`bigstore.test.ts`);
// a timeline keeps every mark until it is cleared, so each name is marked once.

export namespace phases {
  const marked = new Set<string>()

  export const mark = (name: string): void => {
    if (marked.has(name)) return
    marked.add(name)
    performance.mark(name)
  }
}
export default phases
