// What the box answers is data (numbers, strings, booleans, null, arrays and plain records) and what the tab builds of
// it, sets and maps; its equality is structural, and anything else equals only itself.

export namespace data {
  const plain = (x: object): boolean => Object.getPrototypeOf(x) === Object.prototype || Object.getPrototypeOf(x) === null

  export const same = (x: unknown, y: unknown): boolean => {
    if (Object.is(x, y)) return true
    if (typeof x !== "object" || typeof y !== "object" || x === null || y === null) return false
    if (x instanceof Set || y instanceof Set) return x instanceof Set && y instanceof Set && x.size === y.size && [...x].every(v => y.has(v))
    if (x instanceof Map || y instanceof Map) return x instanceof Map && y instanceof Map && x.size === y.size && [...x].every(([k, v]) => y.has(k) && same(v, y.get(k)))
    if (Array.isArray(x) || Array.isArray(y)) return Array.isArray(x) && Array.isArray(y) && x.length === y.length && x.every((v, i) => same(v, y[i]))
    if (!plain(x) || !plain(y)) return false
    const kx = Object.keys(x)
    const ky = new Set(Object.keys(y))
    return kx.length === ky.size && kx.every(k => ky.has(k) && same(Reflect.get(x, k), Reflect.get(y, k)))
  }
}
export default data
