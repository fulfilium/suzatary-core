{
  description = "A secretary's user repository: the data a Suzatary deployment keeps, and the app it runs";

  # The app, followed by branch; every deploy is a commit moving the lock (the
  # app's scripts/deploy.sh). A git input, not github:, because a deploy pins a
  # branch at a revision.
  #
  # Read for its lock alone: evaluating it copies this repository, data and all,
  # into the world-readable Nix store, so it has no outputs.
  #
  # The placeholder address answers nothing; name the https URL you fetch the app
  # from: `sed -i "s|https://example.invalid/suzatary-core|<that URL>|" flake.nix`.
  inputs.suzatary.url = "git+https://example.invalid/suzatary-core?ref=master";

  outputs = _: { };
}
