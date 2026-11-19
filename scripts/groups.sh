# Process groups a script stops whole, by id. The sourcing script sets `group_grace`, the seconds
# from TERM to KILL.

# The groups not yet stopped, by id (the leader's pid).
groups=()

# group_start <command...>: <command> in the background as a group, stdin /dev/null; sets `group`.
group_start() {
  set -m
  "$@" </dev/null &
  group=$!
  set +m
  groups+=("$group")
}

# group_wait <id>: the leader's status; the rest of its group is stopped.
group_wait() {
  local code=0
  wait "$1" || code=$?
  group_stop "$1"
  return "$code"
}

# group_stop <id>...: TERM to each group, KILL to what still runs after the grace. Never `wait`: a
# leader bash reaped before a trap noted it blocks a `wait` until another child ends.
group_stop() {
  local id tries kept=()
  for id; do kill -TERM -- "-$id" 2>/dev/null || true; done
  for id; do
    tries=0
    while group_runs "$id" && ((tries++ < group_grace * 10)); do sleep 0.1; done
    kill -KILL -- "-$id" 2>/dev/null || true
  done
  for id in "${groups[@]}"; do [[ " $* " == *" $id "* ]] || kept+=("$id"); done
  groups=("${kept[@]}")
}

# group_runs <id>: whether a process of the group runs (`kill -0` counts zombies).
group_runs() {
  local stat state pgid
  for stat in /proc/[0-9]*/stat; do
    read -r stat 2>/dev/null <"$stat" || continue
    read -r state _ pgid _ <<<"${stat##*) }"
    [[ "$pgid" != "$1" || "$state" == Z ]] || return 0
  done
  return 1
}
