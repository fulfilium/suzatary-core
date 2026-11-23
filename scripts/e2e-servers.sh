# shellcheck shell=bash disable=SC2154
# The browser lane's servers, by name. NAME's files in $work: argv (NUL-separated), log, pid, from
# (the log's length at the launch) and answer (the last probe's).

# Neither gone nor a zombie.
running() { # pid
  local stat
  stat="$(cat "/proc/$1/stat" 2>/dev/null)" || return 1
  stat="${stat##*) }"
  [[ "${stat%% *}" != Z ]]
}

# In a session of its own, which cleanup kills whole. The bash before the server logs how it ended
# (137 is SIGKILL, e.g. the OOM killer; 134 an abort).
launch() { # name
  local name=$1 argv
  mapfile -d '' -t argv <"$work/$name.argv"
  wc -c <"$work/$name.log" 2>/dev/null >"$work/$name.from" || echo 0 >"$work/$name.from"
  # A subshell, so the server is no child of this shell and is reaped when it ends.
  (
    # shellcheck disable=SC2016
    setsid bash -c '"$@"; echo "==> $0 ended: exit $?"' "$name" "${argv[@]}" >>"$work/$name.log" 2>&1 &
    echo "$!" >"$work/$name.pid"
  )
}

since_launch() { tail -c "+$(($(cat "$work/$1.from") + 1))" "$work/$1.log"; }

ready_within=300

# Waits until `READY NAME` succeeds while NAME runs (a gone server's port answers for another), at
# most $ready_within s. An end on `Address already in use` gets a fresh port by `RESTART NAME`, 4
# tries in all; any other end fails at once. READY must return within seconds: a taken port may
# accept a probe and never answer. `slept` counts its seconds in tenths: 2 a look, 100 a probe
# cut short.
up() { # name ready [restart]
  local name=$1 ready=$2 restart=${3:-} try=1 tries=4 started=$SECONDS slept=0
  until "$ready" "$name" && running "$(cat "$work/$name.pid")"; do
    if ! running "$(cat "$work/$name.pid")"; then
      if [[ -n "$restart" ]] && ((try < tries)) && since_launch "$name" | grep -q 'Address already in use'; then
        try=$((try + 1))
        echo "==> $name: its port was taken before it bound; started again on a fresh one (try $try of $tries)" >&2
        "$restart" "$name"
        continue
      fi
      down "$name" "ended before it was ready, after a wait of $((SECONDS - started)) s"
    fi
    if ((slept >= ready_within * 10)); then
      down "$name" "was not ready in $((SECONDS - started)) s and still runs"
    fi
    sleep 0.2
    slept=$((slept + 2))
  done
}

# GETs URL into NAME.body and its status and time, or curl's words, into NAME.answer; true on 200.
probe() { # name url [curl flags…]
  local answer code=0
  answer="$(curl -sS --max-time 10 "${@:3}" -o "$work/$1.body" -w '%{http_code} in %{time_total} s' "$2" 2>&1)" \
    || code=$?
  echo "$answer" >"$work/$1.answer"
  ((code != 28)) || slept=$((${slept:-0} + 100))
  [[ "$answer" == "200 in "* ]]
}

# A server that binds a port the system picks prints `{"base":"<url>"}` once it listens.
announced() { grep -q '^{"base":"' "$work/$1.log"; }
announcement() { sed -n 's/^{"base":"\([^"]*\)"}$/\1/p' "$work/$1.log" | head -n 1; }

down() { # name why
  {
    echo "web-e2e.sh: the server $1 $2"
    [[ ! -e "$work/$1.answer" ]] || echo "  its last probe was answered: $(cat "$work/$1.answer")"
    echo "  the last 40 lines of $1.log:"
    tail -n 40 "$work/$1.log" | sed 's/^/    /'
  } >&2
  exit 1
}
