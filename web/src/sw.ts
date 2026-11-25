// One bundle beside each build's app: its build is in its script's URL, and its chunks were named when that build was
// bundled (`scripts/build-web.sh`).

import shell from "./app/shell"
import worker from "./app/worker"

if (worker.isScope(self)) {
  const me = self
  shell.fold(shell.buildOf(me.location.href), { stamped: id => worker.run(me, id, __CHUNKS__, __CONTENT__), dev: () => undefined })
}
