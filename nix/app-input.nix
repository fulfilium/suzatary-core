# The app a user repository runs: the `suzatary` input its flake.nix declares
# (WHERE it comes from) and the commit its flake.lock names (WHICH version runs).
#
# A function of those two files alone, so reading it never copies the user
# repository (its stores) into the Nix store.
#
#   nix eval --raw --file nix/app-input.nix --argstr lock <user>/flake.lock source.outPath
#   nix eval --raw --file nix/app-input.nix --argstr flake <user>/flake.nix --argstr rev <sha> [--argstr ref <branch>] pinned
{ lock ? null, flake ? null, rev ? null, ref ? null }:
let
  need = what: arg: if arg == null then throw "${what} needs --argstr ${what}" else arg;
  nodes = (builtins.fromJSON (builtins.readFile (need "lock" lock))).nodes;
  input = nodes.${nodes.root.inputs.suzatary};
  # Imported as a plain attribute set, so no output is evaluated and nothing beside it is copied.
  declared = builtins.parseFlakeRef (import (need "flake" flake)).inputs.suzatary.url;
  branch = if ref != null then ref else declared.ref or "master";
in
{
  # In the store already when the lock was written on this box: the narHash
  # names it, so nothing is fetched.
  source = builtins.fetchTree input.locked;
  # Read by the roots run against the followed branch (`scripts/followed.sh`).
  revision = input.locked.rev;
  lockedUrl = input.locked.url;
  lockedRef = input.original.ref or "master";

  # scripts/deploy.sh resolves a rev at this remote itself, since Nix's fetch
  # answers from its cache any rev it has ever fetched.
  remote = declared.url;
  inherit branch;

  pinned =
    if declared.type != "git" then
      throw "the suzatary input is a ${declared.type} input; a deploy pins a git one"
    else
      "git+${declared.url}?ref=${branch}&rev=${need "rev" rev}";
}
