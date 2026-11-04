# suzatary-plugin-fence

A hostile source plugin, used only by `app/plugin/tests/fence.rs`: each call tries one attack on
the plugin host's fences. The app never loads it.

## Use

The cursor names the mode and its argument (`<mode> <argument>`). With the `escapee` feature it
targets a world that also imports a filesystem and a network, which the host must refuse to load.

## Depends on

`wit-bindgen` alone.

## Develop

The flake builds the component (`.#suzatary-plugin-fence`); the workspace only lints the library.
