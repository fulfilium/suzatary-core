# suzatary-secretary

One turn of the secretary or of the triage agent, pure: no I/O, no async and no clock of its own.
A caller passes the role's record of effects, and the record decides what the turn can do.

## Use

- `agent::Agent` is the triage agent's record. It has no field that sends mail, notifies or
  names a peer, and its `compile_fail` doc tests pin that.
- `secretary::Secretary` is the secretary's record. Its verbs propose, and only the owner's tap
  performs a proposal.
- Under the `nothing` feature, each record has an inert `nothing()`, and `secretary::live_shape`
  builds a memory of the live shape over `suzatary-stores`' stores held in memory, for tests.

## Develop

`cargo test -p suzatary-secretary`. The laws are in `tests/` (proptest).
