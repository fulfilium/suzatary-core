# suzatary-secretary

One turn of the secretary or of the triage agent, pure: no I/O, no async and no clock of its own.
A caller passes the role's record of effects, and the record decides what the turn can do.

## Use

- `agent::Agent` is the triage agent's record. It has no field that sends mail, notifies or
  names a peer, and its `compile_fail` doc tests pin that.
- `secretary::Secretary` is the secretary's record. Its verbs propose, and only the owner's tap
  performs a proposal.
- `stores` is the `stores!` table: every store the data tree keeps, its tier, hold and writer.
  `asks` is the schema of the box's refusals of what a tab posted.
- Under the `nothing` feature, each record has an inert `nothing()`, and `stores` has the stores
  held in memory, for tests.

## Develop

`cargo test -p suzatary-secretary`. The laws are in `tests/` (proptest).
