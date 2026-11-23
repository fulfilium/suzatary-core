// What the worker or a write says to the owner, in the box's words; the toast is imported lazily so the shells stay
// testable without a DOM.

export const told = (said: string): void => {
  void import("../view/toast").then(module => module.toast(said))
}
