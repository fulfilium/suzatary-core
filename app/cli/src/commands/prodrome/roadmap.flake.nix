{
  description = "A Prodrome store of a project's tasks, and the application that reads it";
  # The reader, at the revision the lock records. `nix run . -- todos open --store .`
  inputs.suzatary.url = "git+https://github.com/fulfilium/suzatary-core?ref=master";
  outputs = { suzatary, ... }: {
    apps = builtins.mapAttrs (_: apps: { inherit (apps) default; }) suzatary.apps;
  };
}
