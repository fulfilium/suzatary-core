# My secretary

This repository is a Suzatary
deployment's own: the data it keeps and the exact app it runs. Keep it
private. One commit of it names the whole state, so restoring is a checkout
and a build, and going back is a checkout of an earlier commit.

| | |
|---|---|
| `flake.nix`, `flake.lock` | the app, pinned: the lock names the commit every unit runs |
| `suzatary` | the app the lock names, its tools (`./suzatary tool …`, `./suzatary shell`) and its secrets (`./suzatary secret NAME`), reached without copying this repository into the Nix store |
| `instance.toml` | the deployment, written by `suzatary init`: who it serves, who may open it, the mail accounts, the model |
| `events/`, `events-suzatary/` | your todos and the system's own work, each a content-addressed history |
| `inbox/`, `secretary-lines/`, `memory/`, `decisions/`, `state.json` | the inbox, the conversations, the memory's history, the recorded decisions, the mail queue's bookkeeping |
| `local/` | never committed (its own `.gitignore` ignores everything in it, written by `suzatary init`), carried by the vault: `persona.toml` (your email voice, news filter and anchors), `personal-terms.txt` and `context-terms.txt` (what the leak guard looks for), `mail.json` (what the box keeps of each message beside the decision), `triage-notes.md` (your rules for the triage agent, appended to its prompt: which senders never make a todo, which chain a kind of mail belongs to, what a deadline costs; never facts about people) |

Mail, the memory's words, the rendered pages and the plugins' caches stay on
the box (`.gitignore`); they are rebuilt or deliberately unrecoverable.

## Beginning

```console
$ nix flake init -t git+<app>#user && chmod +x suzatary
$ sed -i "s|https://example.invalid/suzatary-core|<app>|" flake.nix
$ git init -b master && git add -A && nix flake lock        # pins the app
$ ./suzatary init                                 # asks who you are
$ git add -A && git commit -m "Begin"
```

`<app>` is the https URL of the repository you fetch the app from.
`init` asks your name, your timezone, your box's MagicDNS name, your
Tailscale logins and your first mail account, writes `instance.toml`, begins the stores, and prints what only
you can do next: the model's token typed at `secretspec`'s prompt, the
account's sign-in, and `./suzatary doctor` to read it all back. No secret is
ever an argument or a line in this repository.

## Running it

On NixOS, take `nixosModules.default` from the app's flake
(`git+<app>`) in your system's configuration, and
set

```nix
services.suzatary = {
  enable = true;
  user = "ada";
  userRepo = "/home/ada/secretary";   # this checkout
  web.tlsDomain = "ada-box.tailnet.ts.net";   # the name you gave init
};
```

then `systemctl --user start suzatary-roots` once. The units run the app this
repository's lock names, commit what the box writes here every five minutes,
and push it to this repository's remote.

**The tailnet is the only door.** The app listens on your box's Tailscale
address; it opens for the logins `instance.toml` names, and nothing makes it
reachable from the internet. Do not put a public endpoint in front of it:
the box holds your mail.

## Updating

A deploy is a commit here that moves the lock: from a checkout of the app,
`scripts/deploy.sh <rev>` pins it, waits for the units to build and restart,
and pushes. A rollback is a deploy of an earlier revision.

The full guide is the app's own `nix build .#docs`.
