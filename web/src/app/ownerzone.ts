// The owner's zone as this device last heard it from the box, so a cold start offline prints his days, not UTC's.

import kv from "./kv"
import time from "./time"

export namespace ownerzone {
  const KEY = "suzatary.zone"

  // A stored name is input from outside the program: a zone only where this browser's Intl reads it as one.
  export const parse = (name: string | null): time.Zone | null => (name === null || name === "" ? null : time.zoneOf(name))

  /** The zone a cold start reads days in: the last the box named, or UTC until it names one. */
  export const remembered = (): time.Zone => parse(kv.local.get(KEY)) ?? time.UTC

  export const remember = (zone: time.Zone): void => kv.local.set(KEY, zone.name)
}
export default ownerzone
