# suzatary-plugin-ics

The reference calendar plugin: a `source` component that reads each calendar's secret iCal
link through the host and emits one `ics.schema` event per occurrence in a window around today.

## Use

The host loads it as a component; the binary's `suzatary plugin sync` runs it. The manifest
declares each calendar as a credential (`PLUGIN_ICS_*`), so the link never enters the plugin's
memory. `item` is the event it emits, and `sync` the pure step from a feed to items.

## Depends on

The Prodrome, and `wit-bindgen` for the component glue in `guest`.

## Develop

`cargo test -p suzatary-plugin-ics` tests the plain Rust natively; the flake builds the component.
