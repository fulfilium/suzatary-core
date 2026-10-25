# AGENT.md: setting up a private Suzatary

This file is for a coding agent (Claude Code today) running on a person's own Linux box. It
takes them from nothing to a private, running secretary, one checked step at a time. Read
it all before the first command. Then work through it in order, and say each step to the
person before you run it.

**How to work.** Each step ends with a check: the commands to run and what they must
print. Do not start the next step until the check passes. When a check fails, stop and
read [Troubleshooting](#7-troubleshooting). Never work around a failed check.

Below, `<upstream>` is the app's repository: the one this file came from. Find its URL
with `git remote get-url upstream`, or `origin` in a fresh clone. The person's own
repository is called *the user repository*. Examples use Ada, `her@example.invalid` and a box
named `ada-box.tailnet.ts.net`. Replace each one with the person's own answer, and never
write their real values into this file or into anything you send upstream.

## Quick install

These are the six things the person does with their own hands, in order. You do every
other step, and say each one before you run it. The times are rough guides.

| # | What the person does | Where | About |
|---|---|---|---|
| 1 | Install NixOS on the box, turn on flakes and add four tools ([§1](#1-what-the-person-needs-and-why)) | the box's system configuration | an hour, if NixOS is new to them |
| 2 | Put the box and the phone on Tailscale, and turn on MagicDNS and HTTPS certificates (§1) | the Tailscale admin console and the phone | 15 minutes |
| 3 | Answer `init`'s questions: name, time zone, logins, the box's name, the first mail address ([§4.4](#44-init)) | the box's terminal | 5 minutes |
| 4 | Sign in to the mail account, and to the model or skip it for the app's card ([§4.5](#45-the-persons-own-steps)) | the box's terminal, or later the phone | 10 minutes |
| 5 | Add the module to the system configuration and rebuild ([§4.8](#48-the-units)) | the box's system configuration | 10 minutes |
| 6 | Trust the phone: run `./suzatary device pair` in their own terminal and type its code on the phone ([§4.10](#410-the-phone)) | the box's terminal, then the phone | 2 minutes |

The rest of this file is the long form: what is needed and why, the rules, the choices,
then each step with its check.

## 1. What the person needs, and why

Check that each of these is in place before any install. Say plainly that the first two
take some effort: there is no shortcut for them today.

1. **A Linux box they control, running NixOS, that stays on.** A small home server, an
   old laptop or a VM they own will do. *Why:* the box holds their mail, keys and history,
   so it must be theirs. The background jobs are a NixOS module, so NixOS is what keeps
   them running after a reboot.
   - Check: `cat /etc/os-release` names NixOS.
2. **Nix with flakes, and four tools.** On NixOS, add this to the system configuration
   and run `sudo nixos-rebuild switch`:
   ```nix
   nix.settings.experimental-features = [ "nix-command" "flakes" ];
   environment.systemPackages = with pkgs; [ git jq pass gnupg ];
   ```
   *Why:* the app, its tools and its deploys are all Nix builds pinned by a lock file.
   `pass` and `gnupg` hold the secrets (§4.3).
   - Check: `nix flake --help` answers, and `pass version` prints a version.
3. **A Tailscale account, with the box and their phone on it.**
   1. Sign up at <https://login.tailscale.com/start>.
   2. On the box, add `services.tailscale.enable = true;`, rebuild, then run
      `sudo tailscale up` and sign in from the link it prints.
   3. In the admin console's DNS page (<https://login.tailscale.com/admin/dns>), turn on
      **MagicDNS** and **HTTPS Certificates**. Say this to the person: a certificate is
      published in the public Certificate Transparency logs, so the box's MagicDNS name
      (not its address, and nothing it serves) becomes public. Choose a box name that
      says nothing about them.
   4. Install Tailscale on the phone and sign in to the same tailnet. If any mail account
      is Gmail, do the same on a laptop with a browser (§4.5 signs in through it).

   *Why:* the tailnet is the only door. The app listens on the box's tailnet address and
   opens only for the logins the person names. Tailscale says who each visitor is, so the
   app needs no passwords. Nothing is ever exposed to the public internet, and nothing in
   this guide may change that.
   - Check: `tailscale status --json | jq -r .Self.DNSName` prints a name ending in
     `.ts.net.` (for Ada, `ada-box.tailnet.ts.net.`). `tailscale whois` on the phone's
     address names the person's login.
4. **A coding agent on the box.** This is you. Claude Code installs from
   <https://code.claude.com/docs>. *Why:* setup, deploys and connectors are commands and
   code, and the person should not have to type them all by hand.
5. **A model for the secretary.** Either a Claude subscription (the default) or an API
   key. *Why:* the secretary and the mail triage answer with it. See
   [the choices](#3-the-choices).
6. **SSH into the box from that laptop**, if any mail account is Gmail: either
   `services.openssh.enable = true;` or Tailscale SSH (`sudo tailscale up --ssh`). *Why:*
   Gmail's sign-in finishes on the box's `localhost`, which the laptop reaches through an
   SSH tunnel (§4.5).
   - Check: `ssh ada-box.tailnet.ts.net true` from the laptop exits 0.
7. **Somewhere private to keep the user repository**, if they want a copy off the box
   (the default): any git remote they own, set to private. *Why:* one commit of it is the
   whole state, so a remote is the backup and the way back to any earlier state.

## 2. The rules, and why each one holds

These are the deployment's own rules (`CLAUDE.md` holds the full set). Keep them even if
the person asks you not to. When something seems to need breaking one, stop and explain
why instead.

- **Private by default.** The box serves only on the tailnet, to the logins the instance
  names. Never add a public endpoint, a port forward, a tunnel or a Funnel. *Why:* the
  box holds every message the person has received.
- **Every outward act is a proposal the owner taps.** A message to send and an account's
  sign-in are cards in the app's Inbox, performed only by the person's tap. A connector
  is admitted only on their explicit confirmation (§5). Never send mail for them, and
  never write a tool that does. *Why:* the
  triage agent reads attacker-written mail all day, so the one thing it must never reach
  is a way out. That holds only if nothing else adds one.
- **Secrets only through secretspec.** A secret goes in at a prompt only:
  `./suzatary secret NAME` from a terminal, or a card's write-only field in the app. A
  secret never goes in a file, a commit, an argument, a URL, a transcript or this chat.
  Never ask the person to paste one to you. If a step seems to need its value, the step
  is wrong. *Why:* a value in a chat or a commit cannot be scrubbed afterwards.
- **The data lives in a repository the person chooses.** The user repository holds the
  instance file, the histories and the pin of the app. Ask where its remote should be;
  never pick one for them. *Why:* it is their data, and only a commit of it can be
  restored.
- **No backward compatibility.** When a stored format changes, the app ships a migration
  verb. Run it on a copy first, then on the box, and then the old form is deleted. Do not
  keep both, and do not hand-edit stored objects. *Why:* any earlier state can still be
  checked out of the user repository's history together with the code that read it.

### The app runs unchanged

A person runs Suzatary's core **unchanged**. Their user repository pins it by commit (the
`suzatary` input of its `flake.nix`). That repository holds everything that is theirs:
the instance file, their connectors, their notes and their data.

- **Customise through configuration and connectors only.** Never edit the core's code
  for one person. There is no private fork to keep in step.
- **When the core is wrong** (a bug, a missing guard, a connector others could use), fix
  it upstream. Follow [§6](#6-when-the-core-needs-a-fix).
- **When the person wants something that neither configuration nor a connector can
  express**, that is a gap in the core. Propose it upstream as an issue or a pull request
  that describes the need. Do not patch it locally.

### What leaves your box

Every place this program sends anything, what it is handed, and how to send
it nothing. The list is the code's own: `suzatary doctor --outbound` prints
it from the constants the box sends by, and a guard fails while this text
differs.

<!-- outbound: generated by `suzatary doctor --outbound`; edit the code, never this text -->

- **The model vendor: Anthropic, through the `claude` CLI on the Claude subscription.**
  Receives: Whatever a job puts in its prompt: the secretary's conversations and what its verbs read (mail bodies, todos, calendar titles, files you attach), the triage agent's mail and the open list, the manuals, your persona's prose and your triage notes. Every `claude` the box starts keeps no session it was not asked to and runs with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `DISABLE_TELEMETRY=1`, `DISABLE_ERROR_REPORTING=1`, `DISABLE_GROWTHBOOK=1`, `DISABLE_AUTOUPDATER=1`, `DISABLE_BUG_COMMAND=1`, `DISABLE_FEEDBACK_COMMAND=1`, `CLAUDE_CODE_DISABLE_FEEDBACK_SURVEY=1`, `DO_NOT_TRACK=1`.
  Off: Set `[secretary] model` and `[secretary] compactor` in the instance file to choose which Claude model each job runs, and `[secretary] compact = false` to stop the compactor. There is no secretary without a model.

- **The data repository's git remote (GitHub, or wherever you point it), on every push `safe-push.sh` makes.**
  Receives: The stores: the todo chains (each todo's text, price and history, and `mail:` references by hash prefix), the inbox (each proposal whole: a drafted email's recipients, subject and body), the conversations (each line's shape, its words sealed), the memory's and the digest's histories (names, never the words), the decisions, `requests/` (who asked for each write) and `state.json` (how many messages were decided and how, each under an HMAC no one without SUZATARY_DEDUP_KEY can tie to a message); `instance.toml` (your name, city, zone, logins, accounts and the app's address, which names the box) and `FETCHER` (the box's hostname). Never the box-local tier, `local/`: `persona.toml`, `personal-terms.txt`, `context-terms.txt`, `mail.json`, `triage-notes.md`, `watches`. The repository's earlier commits keep whatever it held before: moving a file into the tier stops it being pushed, and rewrites no history.
  Off: Leave the data repository without a remote; the box then keeps its history alone, and the vault is the only copy off it.

- **Notifications: Firebase Cloud Messaging (`https://fcm.googleapis.com`) to each registered phone, and ntfy (`https://ntfy.sh`) on your topic.**
  Receives: A heading, a line and a link into the app at your box's address, and nothing else; each push is one of: "Suzatary" / "1 new or updated todo and 1 change from the agent" → `https://<box>/#/`; "Suzatary needs you" / "1 thing is waiting for you" → `https://<box>/#/inbox`; "Your morning brief" / "The brief for 2026-01-02 is ready." → `https://<box>/#/briefings/2026-01-02`; "No morning brief" / "This morning's brief was not written; the box's log says why." → `https://<box>/#/secretary`; "The secretary answered" / "Open the conversation to read it." → `https://<box>/#/secretary/2026-01-02-1`; "Conversations do not read" / "1 conversation the box cannot read; the review says which" → `https://<box>/#/secretary/2026-01-02-review`; "The mail you were waiting for" / "A message you asked to hear about has arrived." → `https://<box>/#/mail/0123456789abcdef`; "The secretary's review is down" / "1 card waiting; it has failed 1 pass running" → `https://<box>/#/inbox`.
  Off: Leave out `[notify]`, or name only the channels you want.

- **Calendar hosts: each origin a calendar plugin's manifest allows.**
  Receives: A GET of the secret link you set; nothing of the box.
  Off: Declare no `[[plugins]]`.

- **The weather: `https://wttr.in`.**
  Receives: Your `[persona] city`, by name, when a brief asks for the forecast.
  Off: Leave `city` empty.

- **The vault: your Proton Drive.**
  Receives: Weekly, an archive encrypted to a key whose private half is off the box (the pass store, the keys, the grants, the memory's words, the box-local tier); and a file you choose to keep private, at `/my-files/Suzatary/Private`.
  Off: Disable the vault timer and set no `drive_cli`; then a lost box loses what only it held.

- **The coding routine: `https://api.anthropic.com`, the one Claude Code routine `[coding]` names.**
  Receives: The task text of a coding session you tapped, and nothing else.
  Off: Leave out `[coding]`.

- **Your mail providers, each account's own IMAP and SMTP.**
  Receives: The sync's logins, and a message only when you tap Send on it.
  Off: Remove the account from `[[mail.accounts]]`.

<!-- end of outbound -->

## 3. The choices

Ask these one at a time, in this order. Say what the default is, and record each answer
before asking the next.

1. **Where the data and the code live.** By default the data lives in a git repository
   with a private remote of their choosing, and the code is `<upstream>`, pinned by
   commit. The remote can be any forge, or none (the repository stays only on the box).
   *Ask:* where the remote is, or whether they want none.
2. **Which Claude model answers.** Every job runs a Claude model through the `claude`
   CLI on the owner's Claude subscription; `[secretary] model` and `[secretary]
   compactor` name the models. The box asks for a missing token itself: a card in the
   Inbox, and **Connect a model** whenever the model does not answer.
3. **Their first mail account**: its address and its provider. More accounts can be added
   later.
4. **Calendars.** One secret iCal link per calendar, set after the install.
5. **Storage**, meaning a drive the secretary may read: which folders, if any. This is
   the instance's `[secretary] drive_allow`. Leave it empty unless they ask.

The list below is generated from the app itself (`suzatary init --options`), and a test
keeps this file equal to it. The flags, mail providers and worlds it names are exactly
the ones the app accepts.

<!-- options: generated by `suzatary init --options`; edit the code, never this text -->

### Who, where, and who may come in

`init` asks for any of these it is not given, at a terminal.

| Flag | What it sets |
|---|---|
| `--name` | Your name, as the secretary addresses you (`[persona].name`) |
| `--zone` | The time zone you live in, by name (`America/New_York`); yours, so change it when you move |
| `--city` | The city `suzatary weather` forecasts; none when absent |
| `--actor` | The actor your own writes carry; your first name in lower case when absent |
| `--box` | This box's `MagicDNS` name, `Self.DNSName` in `tailscale status --json`: the app answers at `https://<it>` |
| `--login` | A Tailscale login of yours: it opens the app and writes as you. Repeatable, at least one |
| `--category` | A category of todos, most important first. Repeatable; work, study, project, home and other when absent |
| `--email` | Your first mail account's address; more are added to the file by hand |
| `--mail-provider` | Who serves that account |
| `--repo` | The code root, relative to the instance file |
| `--data` | The data root (the chains, `state.json`, the conversations), relative to the instance file |

### Mail

One account at `init` (`--email` and `--mail-provider`); more are added to `[[mail.accounts]]` later. Each provider leaves a sign-in only the person can do:

- `gmail`: Gmail, through lieer

  ```console
  $ ./suzatary tool sh -c 'mkdir -p Mail/her-at-example-invalid && cd Mail/her-at-example-invalid && gmi init her@example.invalid'   # lieer's sign-in, in a browser; on a box with no browser, open the SSH tunnel in AGENT.md §4.5 first
  ```

- `microsoft-work`: A work or school Microsoft 365 account, over IMAP

  ```console
  $ ./suzatary auth her@example.invalid   # the device-code sign-in, for reading
  $ ./suzatary auth her@example.invalid --scope smtp   # a separate consent, for sending
  ```

- `microsoft-personal`: A personal Outlook.com account, over IMAP

  ```console
  $ ./suzatary auth her@example.invalid   # the device-code sign-in, for reading
  $ ./suzatary auth her@example.invalid --scope smtp   # a separate consent, for sending
  ```

- `proton`: Proton Mail, through its Bridge

  ```console
  $ ./suzatary tool protonmail-bridge --cli   # `login`, then `info` for the Bridge password
  $ ./suzatary secret PROTON_BRIDGE_PASSWORD   # that password, typed at the prompt
  $ $EDITOR mbsyncrc   # a `proton` group pulling Inbox
  ```

### Model

Every job runs a Claude model through the `claude` CLI on the Claude subscription: `[secretary] model` for the chat, the reviews and the titles, and `[secretary] compactor` for the compactor. Its token is set at a terminal as below, or, once the app runs, from the **Connect a model** card the box files in the Inbox whenever the model does not answer.


  ```console
  $ ./suzatary tool claude setup-token   # a year's token for the unattended jobs; or skip this step and the next: the app asks you to connect the model, from your phone
  $ ./suzatary secret CLAUDE_CODE_OAUTH_TOKEN   # typed at the prompt, never on the command line
  ```

### Calendars and other connectors

A connector is a plugin: a WebAssembly component of the `source` world, built for `wasm32-wasip2` and declared by a `[[plugins]]` table that names every origin it may reach and every credential it may name. A credential is a secret link the host fetches for it. The app ships the calendar (`ics`, one secret iCal link per calendar, each a credential named `PLUGIN_ICS_<LABEL>`). The host links one import of its own, `suzatary:plugin/fetch`, and of WASI only the clocks, random numbers and standard streams.

<!-- end of options -->

## 4. The install

Run every command as the person's own user, not as root, unless it says `sudo`.

### 4.1 A checkout of the app

You need a checkout of `<upstream>`, for its deploy script and for fixes.

```console
$ git clone <upstream> ~/src/suzatary && cd ~/src/suzatary
$ git remote rename origin upstream
```

- Check: `git -C ~/src/suzatary log -1 --format=%H` prints a 40-character commit.

### 4.2 The user repository

Start the user repository from the app's template. The template's `flake.nix` names the
app at a placeholder address that nothing answers, so it cannot be locked until the
real URL is in place. `<upstream>` must be an https URL here.

```console
$ mkdir ~/secretary && cd ~/secretary
$ nix flake init -t "git+<upstream>#user" && chmod +x suzatary
$ sed -i "s|https://example.invalid/suzatary-core|<upstream>|" flake.nix
$ git init -b master && git add -A && nix flake lock
```

- Check: `./suzatary which` prints `git+<upstream>?ref=master&rev=<40 hex>`, and
  `./suzatary --help` lists the verbs, `init` among them.

### 4.3 Where secrets are kept

secretspec keeps each secret in `pass`, which encrypts it to a GPG key on the box. The
units run unattended, so that key has **no passphrase**: nothing could type one at 3 in
the morning. **Say the trade-off to the person plainly.** Anyone who can act as their
user on the box, or read its disk, can read every secret. The box itself is the
boundary, so it should have full-disk encryption and no other users. They must agree to
this before you continue.

```console
$ gpg --batch --passphrase '' --quick-gen-key "Ada's box <box@example.invalid>" default default never
$ pass init "$(gpg --list-secret-keys --with-colons | awk -F: '/^fpr/ {print $10; exit}')"
$ ./suzatary tool secretspec config global init
```

At secretspec's prompt, choose `pass`.

- Check: `pass ls` prints `Password Store`, and `./suzatary tool secretspec config
  global show` names `pass`.

### 4.4 `init`

Run `init` with the answers from §3. Anything you leave out is asked for at the terminal.
For Ada:

```console
$ ./suzatary init --name Ada --zone Europe/London --box ada-box.tailnet.ts.net \
    --login her@example.invalid --email her@example.invalid --mail-provider gmail
```

- Check: it prints `wrote …/instance.toml` and then `next, at a terminal:`, followed by
  the steps that only the person can do. `ls` shows `events/` and `events-suzatary/`.

### 4.5 The person's own steps

Hand each step that `init` printed to the person, one at a time: the model's token or
key, typed at secretspec's prompt, and the mail account's sign-in. They type every
value; you never see one. If they skip the model's step, nothing is lost: once the app
runs (§4.8), the box files a **Connect a model** card in the Inbox. Its tap starts the
same Claude sign-in, shows its link, and takes the code in a secure field.

**Gmail on a box with no browser.** `gmi init` opens a sign-in page and waits for
Google to redirect to `localhost:8080` on the box. To finish it from a laptop:

1. On the laptop, open a tunnel to the box:
   ```console
   $ ssh -L 8080:localhost:8080 ada-box.tailnet.ts.net
   ```
2. In that SSH session, `cd ~/secretary`, then run the `gmi init` step `init` printed.
3. Open the URL it prints in the laptop's browser and sign in.

Google's redirect reaches the box through the tunnel. If the URL's `redirect_uri` names
a port other than 8080, use that port in the tunnel instead.

- Check: `./suzatary doctor` lists the account under `mail` with its sync strategy (for
  Ada, `her@example.invalid … gmail (lieer)`). Under `models`, each job with an order is
  followed by `→ <backend>/<model> right now` once its key resolves (`→ nothing usable
  right now` means it does not). With the subscription the orders are empty and every
  job reads `(none — claude-cli/…)`. If its token is not set yet, `doctor` says so and
  names the command; skipping it is fine, since the app asks for it. The token is checked
  by the first triage run in §4.8.
- `doctor`'s `door` line names the app's address, `ada-box.tailnet.ts.net:443`. Run from a
  shell, it adds that the unit binds `services.suzatary.web.port` (443 by default) and that
  a `serve` run by hand would bind 8787. That is expected here, not a failure: the unit
  serves the door, and nothing runs `serve` by hand. Only when `doctor` runs under the unit
  does it say `it will not start`, and then the fix is to set `web.port` to the door's port
  (§4.8).

### 4.6 Signing keys

Every write is signed, and every deployment declares the root it signs under. `init`
(§4.4) already minted the box's keys into secretspec and wrote their root into
`instance.toml`'s `[signatures]` table; nobody types or pastes it. `device init` adopts
it on every chain now, rather than at the first write:

```console
$ ./suzatary device init
```

- Check: `./suzatary doctor`'s `signs` line reads `root <hex>; the box's and the agent's
  keys resolve`.

### 4.7 Commit, and the remote

`FETCHER` names the one box meant to fetch mail. Only `suzatary up` enforces it: it
compares the file with `uname -n` and refuses to enable the units on any other box. The
vault backup only names its snapshot after it. Nothing else enforces it, so never start the
units on a second box by hand.

`local/personal-terms.txt` lists, one term per line, the person's names, every address and
login, and the box's and tailnet's names. It lives in the box-local tier `local/`, which
`init` began with a `.gitignore` of its own that keeps it out of every push; the vault
carries it. The core's leak guard reads it, so nothing sent upstream can name the person
(§6). The person's email voice, news filter and anchors go in `local/persona.toml` beside
it, never in `instance.toml`, and their rules for the triage agent in
`local/triage-notes.md`.

```console
$ uname -n > FETCHER
$ $EDITOR local/personal-terms.txt
$ git add -A && git commit -m "Begin"
$ git remote add origin <their private remote> && git push -u origin master
```

Skip the last line if they chose no remote. That first push is the person's, with their
own credentials; the box's own pushes come in §4.9.

- Check: `git status --short` prints nothing, and `./suzatary verify` exits 0.

### 4.8 The units

Add the app's NixOS module to the box's system configuration. The units always run the
app that the user repository's lock names; the system configuration takes only the
module from the app.

**A flake-based system configuration** (`/etc/nixos/flake.nix`) gets an input, passes
`inputs` to its modules through `specialArgs = { inherit inputs; };`, and imports the
module:

```nix
inputs.suzatary.url = "git+<upstream>";
```

**A plain `/etc/nixos/configuration.nix`** has no inputs. It fetches the module at a
pinned commit instead, and moving that commit is a hand edit, made only when the module
itself changes:

```nix
imports = [ (builtins.getFlake "git+<upstream>?rev=<40-hex commit>").nixosModules.default ];
```

Either way, the options:

```nix
imports = [ inputs.suzatary.nixosModules.default ];   # or the getFlake line above
services.suzatary = {
  enable = true;
  user = "ada";
  userRepo = "/home/ada/secretary";
  web.tlsDomain = "ada-box.tailnet.ts.net";
};
```

That runs the core units and nothing else: the roots refresh, the triage timer, the
plugin sync, the IMAP watcher and the web. The watcher and the sync read the instance at
start, so a box with no Microsoft account and no `[[plugins]]` costs nothing. The rest is
declared in the options, never masked:

- `protonBridge.enable = true;` when an account in `[[mail.accounts]]` has
  `sync = "proton"`. It runs `proton-bridge.service`, restarted always.
- `vault.enable = true;` once the user repository holds `VAULT_RECIPIENT` and the Proton
  Drive CLI at `~/bin/proton-drive` is signed in. It runs the daily encrypted snapshot
  (`suzatary-vault.timer`), keeping the newest seven. A declared vault that cannot upload
  fails the unit every day,
  so declare it only then.

A box set up by an earlier version of this guide masked these two; unmask them after the
rebuild that declares them (`systemctl --user unmask proton-bridge.service
suzatary-vault.timer`), since a mask outranks the module.

`systemctl --user` answers with nothing unless `XDG_RUNTIME_DIR` is set, and
`deploy.sh` refuses to run without it. Export it in every shell you run these commands
from:

```console
$ sudo nixos-rebuild switch
$ export XDG_RUNTIME_DIR=/run/user/$(id -u)
$ systemctl --user start suzatary-roots
```

- Check: `systemctl --user show -p Result suzatary-roots` prints `Result=success`, and
  `systemctl --user is-active suzatary-web` prints `active`. From the phone, with
  Tailscale on, `https://ada-box.tailnet.ts.net` opens the app.

### 4.9 The box's own pushes

Every five minutes the triage unit commits what the box wrote and pushes it to the user
repository's remote, with no one at the keyboard. Skip this step if they chose no
remote. It comes after §4.8 because the module puts `secretspec` on the system's `PATH`,
and the push helper needs it.

- **On GitHub:** the box asks secretspec for a fine-grained token
  (`SUZATARY_DATA_PUSH_TOKEN`), with contents read and write on that one repository and
  nothing else: the repositories the secretary reaches have a token of their own
  (`SUZATARY_REPOS_TOKEN`). The person mints it on GitHub and notes its expiry date, then:
  ```console
  $ ./suzatary secret SUZATARY_DATA_PUSH_TOKEN
  $ git -C <data repository> config credential.https://github.com.helper ''
  $ git -C <data repository> config --add credential.https://github.com.helper "$SUZATARY_ROOTS/app/scripts/git-credential-secretspec.sh"
  ```
  Both lines are the data repository's own, since its token pushes nothing else: the
  empty entry clears the helpers a global config lists (gh's among them) for that
  repository, so this one is the only one asked. The helper sends the username the remote URL names, if
  any, and GitHub's generic token user otherwise.
- **On another forge:** today the helper serves GitHub only. Use an SSH remote with a
  deploy key that has write access to that one repository. The key is a file on the box,
  unencrypted, like the GPG key in §4.3, and the same trade-off applies.

- Check: `git -C ~/secretary push --dry-run` succeeds without asking for anything, and
  within five minutes `journalctl --user -u suzatary-triage -n 50` shows a pass that
  pushed.

What is pushed holds no conversation's words: `secretary-lines/` holds each line's
shape and its words sealed, and the keys that open them, `secretary-lines-keys/`, stay on
the box (the user repository's `.gitignore` lists it) and in the daily vault, so losing
the box and the vault together loses the words. What is pushed in the clear is each line's
shape: who and when, the verbs called, the references and file sizes, and a coding
session's branch name (`claude/<task-slug>`); an erased line's shape stays readable. Any
login the app lets view receives the keys when it pulls, the same people who could read the
conversations before. A device whose key was revoked is answered 410 on a signed read and
deletes what it holds, but it can still pull keys with unsigned reads while its login can
view: reads stay open to any device on the owner's login. Erasing a turn destroys its keys.
What erasing cannot reach is said plainly to the person: a push notification already
shown, the day's `briefings/`, the server's memory until it restarts (the words it keeps
opened, up to 32 MiB of them, are forgotten by the erasure itself), Anthropic's
own transcripts, a conversation's title (written from its first answer, and not offered by
a forget), and the fact that an erased line was there and when. The conversations from
before the words were locked were carried into `secretary-lines/` and their plain store
removed; the repository's earlier commits still hold them in the clear.

### 4.10 The phone

No device is trusted yet, so there is no trusted device to tap a card on. The first one is
trusted with a short code from the box's terminal.

1. On the phone, open the app and go to Settings. **This device** stands first while the
   phone is not trusted.
2. The person runs this in their own terminal on the box. You never run it, so the code
   never appears in your transcript:
   ```console
   $ ./suzatary device pair
   ```
   It prints a code such as `BCDF-GHJK`. The code works once, for 10 minutes, and five
   wrong tries end it. The box keeps only a keyed hash of it, and never logs it.
3. On the phone, tap **Enter the code from the box**, type the code and tap **Trust this
   device**. The box trusts the key that signed that request, and no other.

If the code expired, was used or was mistyped five times, the phone says so: run `device
pair` again for a new one. Every later device can do the same, or ask with **Ask to trust
this device** and be tapped in the Inbox on a device that is already trusted.
`./suzatary device trust <key>` still trusts a key typed whole, and withdraws its card.
Revoking a key is done at the terminal only: `./suzatary device revoke <key>`.

- Check: Settings' **This device** reads `…which the chain trusts`, and adding a todo
  from the phone works.

### 4.11 A calendar

The app ships the calendar connector, `ics`: a `source` that reads secret iCal links. The
person adds a calendar from the app: in Settings, **Ask to add one** opens the Setup
conversation, where they say what to add ("add my Work calendar"). The secretary files a
Connect card; its tap writes the table below into `instance.toml`, and the box then files
a card asking for the calendar's secret link. A mail account and a Drive folder are added
the same way.

The hand edit is the fallback, for a box where no model answers yet. Add the table, one
credential per calendar, and list every origin the links are served from:

```toml
[[plugins]]
name = "ics"
component = "ics.wasm"
schema = "ics.schema"
origins = ["https://calendar.example.test"]
credentials = ["PLUGIN_ICS_WORK"]
```

The running app sees the edit with no restart: the server reads `instance.toml` again
within a second of any change to it, so the calendar's line appears in Settings and the box
files its Secret card. The same holds for a mail account or a `[secretary] drive_allow`
folder added by hand; a new `drive_cli` path is read when the server starts.

The person copies the link from their calendar service's settings (it is usually called
the secret address in iCal format). They set it with `./suzatary secret
PLUGIN_ICS_WORK`, or from the Secret card the box files in the Inbox for any declared
credential that is not set. You never see it. A link stored from the card is checked at
once: the line says "Synced" or why it failed, and a failing link gets a card to set it
again. A link served from an origin not in `origins` gets an Allow card inside the link's
card in Settings; allowing it stores the link the person already pasted.

secretspec sets only a name its file declares, and the app's `secretspec.toml` in the code
root declares no `PLUGIN_*` names. So the app writes a second file under its state
directory, the first time a verb asks for a secret, that extends the code root's and declares every name the instance derives (each
plugin credential, the model's token); `./suzatary secretspec` prints it, and the script's
`secret` and the Secret card both set the link through it. Never write the link into a file.

- Check: `systemctl --user start suzatary-plugins` runs the sync (the unit passes the
  app's own components), `journalctl --user -u suzatary-plugins -n 20` shows no
  refusal, and the app's Calendar page shows the week.

### 4.12 Deploys from now on

A deploy is a commit to the user repository that moves its lock. Run it from the app's
checkout, at a commit on upstream's `master`:

```console
$ cd ~/src/suzatary && git fetch upstream
$ scripts/deploy.sh "$(git rev-parse upstream/master)"
```

`deploy.sh` builds first, runs the new binary's `verify` against the live instance, then
commits, restarts and pushes. If any of that fails, it moves nothing. A rollback is a
deploy of an earlier commit.

- Check: its last line reads `deploy: <rev> is live; …/api/about answers 200 from the
  build the lock names`, and `git -C ~/secretary
  log -1` reads `Deploy <rev>`.

## 5. A connector for a service with no default

A connector is a plugin: a WebAssembly component the host runs under a fence. The
component cannot reach beyond what its world imports and its manifest grants. Write one
only when nothing in §3 fits the service.

1. **Write one function.** A connector exports `sync(cursor, credentials)` from the
   `source` world in `app/wit/plugin.wit`, and returns its items and the next cursor. Its
   one import is `fetch`. It is built for `wasm32-wasip2`, and the host links only WASI's
   clocks, random numbers and standard streams beside it. A component that imports a
   filesystem, a socket or anything else is refused before it runs.
2. **Write the manifest's grant.** A `[[plugins]]` table names every origin the plugin may
   fetch and every credential it may name. A credential's value is a secret URL, which
   the host fetches for the plugin over https or loopback only. Names start with
   `PLUGIN_<NAME>_`, and the person sets each one with `./suzatary secret
   PLUGIN_<NAME>_<LABEL>`. The plugin never holds a value.
3. **Declare its schema.** A `.schema` file beside the component lists the registers of
   what it emits. The host admits it under the Prodrome's laws, or refuses it naming the
   law it breaks.
4. **Build it reproducibly.** Copy the shape of `app/plugin-ics` and its build in
   `flake.nix` (`.#suzatary-plugins`): a `cdylib` for `wasm32-wasip2` with the plugins'
   pinned toolchain. A second build must give the same bytes.
5. **Test it, and know where the fence is.** Test its parsing as ordinary Rust tests in
   its crate, as `app/plugin-ics/tests` does. The fence belongs to the host, not the
   plugin, so a connector is held to it however it was written. `Host::load` refuses a
   component that imports beyond what the host links, and every fetch, credential and
   echoed secret is checked while it runs. `app/plugin/tests/fence.rs` proves those fences
   against a hostile component, and the merge gate runs it. Never weaken a fence to make
   a connector work.
6. **Ship it with the app.** The box reads components from `.#suzatary-plugins`, so a
   connector is a crate under `app/` in that output, offered upstream as in §6.
7. **Admission is the person's.** Show them every origin and every credential name. Only
   once they confirm that exact text does the table go into `instance.toml` and get
   committed. A new origin later means a new confirmation.

- Check: `./suzatary plugin sync <name>` stores what it learnt, and the app shows the
  connector's last run.

## 6. When the core needs a fix

1. Branch off upstream's current `master` in the app's checkout:
   `git fetch upstream && git switch -c fix/<slug> upstream/master`.
2. Apply only the general fix. Leave out the person's names, addresses, configuration
   and data, even as examples. Tests use the neutral cast (`suzatary_core::cast`).
3. Run the gate that `CONTRIBUTING.md` names, and fix whatever it reports.
4. **Scan for private content before any push.** Commit first, then run:
   ```console
   $ tree=$(mktemp -d)/tree && scripts/public-tree.sh "$tree"
   $ SUZATARY_INSTANCE=~/secretary/instance.toml scripts/public-tree.sh --check "$tree"
   $ git diff upstream/master | grep -inwF -f <(grep -v -e '^$' -e '^#' ~/secretary/local/personal-terms.txt)
   ```
   The check must end `is clean`. It covers the committed tree and reads
   `local/personal-terms.txt`, `FETCHER` and the app's address beside the instance. The `grep`
   must print nothing. Then read the diff yourself for any key, token or value from their
   `instance.toml`. If you find anything, do not push: remove it, commit, and scan again.
5. Push the branch and open a pull request upstream with a plain description of the bug
   and the fix.
6. **To run the fix before it lands**, deploy its commit from its own branch:
   ```console
   $ cd ~/src/suzatary && scripts/deploy.sh "$(git rev-parse fix/<slug>)" --ref fix/<slug>
   ```
   `--ref` makes the deploy check the commit against the fix's branch and pin it there;
   the user repository's `flake.nix` keeps following `master`.
7. **Once it lands**, deploy upstream's `master` as in §4.12. No fork remains, so there is
   nothing to keep in step.

## 7. Troubleshooting

Start with what the box says, not with a guess.

| Check | What it says |
|---|---|
| `./suzatary doctor` | what the binary read: the instance, every account, the Claude models, whether the signing keys resolve |
| `./suzatary verify` | whether every store reads: each object rehashed, each parent present |
| `systemctl --user status suzatary-web suzatary-roots` | whether the units run, and which app they run |
| `journalctl --user -u suzatary-web -n 100` | the web's own words; use `-u suzatary-triage` or `-u suzatary-plugins` for the timers |
| `tailscale status`, `tailscale whois <ip>` | whether a device is on the tailnet, and whose it is |

The usual failures:

- **`systemctl --user` prints nothing, or `deploy.sh` refuses at once.** Run
  `export XDG_RUNTIME_DIR=/run/user/$(id -u)`: the user manager is not reachable without
  it.
- **The app does not open on the phone.** Check that the phone is on the tailnet and
  that its login is among `instance.toml`'s `[web]` logins. A stale `tailscale serve`
  rule can hold the port: run `tailscale serve status`, and if a rule is there, `sudo
  tailscale serve reset`.
- **The browser warns about the certificate, or the page serves plain http.** HTTPS
  Certificates are off in the tailnet's admin console, or `web.tlsDomain` does not match
  `Self.DNSName`. Fix the setting, then `sudo systemctl start suzatary-tls`.
- **The app opens but nothing can be written.** The device's key is not trusted yet. For
  the first device, run `./suzatary device pair` and type its code in Settings, This device
  (§4.10). For later ones, do the same or tap the card in the Inbox on a trusted device. A
  request from the box itself only ever reads.
- **`doctor` says a secret does not resolve.** The person taps the card the box filed for
  it in the Inbox, or sets it with `./suzatary secret NAME`. Never set it yourself.
- **The box stops pushing.** A fine-grained `SUZATARY_DATA_PUSH_TOKEN` expires on the
  date chosen when it was minted, and then every push fails. `journalctl --user -u
  suzatary-triage` shows git refusing the credential. The person mints a new token and
  sets it again with `./suzatary secret SUZATARY_DATA_PUSH_TOKEN`. Nothing is lost meanwhile: the commits wait on the box.
- **Mail stops syncing.** Run `journalctl --user -u suzatary-triage`. `NeedsLogin`
  means the account's sign-in lapsed: the person runs `./suzatary auth <address>` again,
  or taps the Inbox card the box files for it.
- **`deploy.sh` refuses.** Each refusal is one sentence ending in `Nothing moved.` Do
  what it says. A rev that is not on the followed branch's first-parent chain is refused
  by design, because only gated commits go live.
- **Services run the old app after a rebuild.** A unit file in `~/.config/systemd/user/`,
  a mask included, overrides the module's unit. Run `systemctl --user show -p
  FragmentPath suzatary-web`, which must point under `/etc/systemd/user/`.
- **A disk is full.** Builds and the triage guard fail quietly. Run `df -h` before you
  believe any other failure.
