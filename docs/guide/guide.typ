= Running your own secretary

Suzatary is a secretary you run on your own machine. It reads your mail, keeps
your todos, and drafts what should be sent, and nothing it writes leaves the
machine until you tap it on your phone. This guide takes you from nothing to a
running deployment. The deployment it shows is the test suite's: Ada and her
made-up correspondents, every address under a reserved domain.

#shot("list-phone.png", [The todo list on a phone, most urgent first.])

== What you need

- A Linux box that stays on, with #link("https://nixos.org/download")[Nix] and
  flakes enabled. NixOS runs the background jobs for you; elsewhere you run the
  same flake outputs under your own scheduler.
- A #link("https://tailscale.com")[Tailscale] network with the box and your
  phone on it. It is the only way into the app.
- A Claude subscription.
- A mail account at Gmail, Microsoft or Proton.

== Two repositories

The app is one repository, and your deployment is another, private one that
pins it. Your repository holds everything that is yours: `instance.toml`, the
stores your todos and conversations live in, your own triage rules. Its
`flake.lock` names the exact commit of the app it runs, so one commit of it
names the whole state: restoring is a checkout and a build, and a rollback is
a checkout of an earlier commit. The app never names you; that is what lets it
be shared.

== The quick start

Begin your repository from the app's template, pin the app, and answer `init`:

```console
$ mkdir secretary && cd secretary
$ nix flake init -t git+<app>#user && chmod +x suzatary
$ sed -i "s|https://example.invalid/suzatary-core|<app>|" flake.nix
$ git init -b master && git add -A && nix flake lock
$ ./suzatary init
```

`<app>` is the https URL of the repository you fetch the app from: the
template's flake names a placeholder that nothing answers until you replace it.
`./suzatary` is how you reach the app from your repository: `./suzatary
<verb>` runs the app your lock names, `./suzatary tool <command>` and
`./suzatary shell` give you its tools (`claude`, lieer's `gmi`, the Proton
Bridge, `secretspec`, `notmuch`), and `./suzatary secret <NAME>` sets a
secret at a prompt. It reads only `flake.lock`; nothing ever evaluates your
repository as a flake, since that would copy your data into the Nix store,
which every user of the machine can read.

`init` asks your name, your timezone, your box's MagicDNS name (`Self.DNSName`
in `tailscale status --json`), your Tailscale logins and your first mail account,
or takes each as a flag. Ada's deployment, the one this guide's
pictures show, was begun with

#demo-command

and that wrote this `instance.toml`, which is the whole of what makes a
deployment hers:

#demo-instance

It also begins the two stores every deployment has, `life` for your own todos
and `suzatary` for the system's own work, and prints what only you can do
next. For Ada that is the model's token and her account's sign-in:

#demo-steps

No secret is ever an argument to any command, a line of `instance.toml` or a
message to the secretary. Each is typed at `secretspec`'s prompt, or into the
app's Settings page, which can set a secret and never shows one back.

Then commit, and give the repository a private remote:

```console
$ git add -A && git commit -m "Begin"
$ git remote add origin <your private repository> && git push -u origin master
```

== The door is your tailnet

The app has no login page, no password and no cookie, on purpose. It answers
on your box's Tailscale address, and Tailscale tells it which login owns each
connection. The logins in `[web] viewers` may open the app; those in
`[web.act_as]` may also write, each as the actor it maps to. A request from
the box itself may read and never write, so neither the box's shell nor the
triage agent can act as you.

*Never put a public endpoint in front of it.* The box holds your mail. Nothing
in the app, the template or the module opens one, and no step of this guide
asks you to.

== The model

Every job runs a Claude model on your Claude subscription through the
`claude` command, with the year-long token `claude setup-token` mints. The
instance file names the models: `[secretary] model` for the conversation,
the reviews and the titles, and `[secretary] compactor` for the compactor.
The Settings page shows which model each job runs.

== Mail

Each account's consent is given once, at a terminal, by you: a background job
can never start a sign-in. `init` prints the one your first account needs.

#table(
  columns: 2,
  [*Provider*], [*Sign-in*],
  [Gmail (`--mail-provider gmail`)],
  [lieer's `gmi init` in the account's mail folder, through `./suzatary tool`],
  [Microsoft 365 (`microsoft-work`) or Outlook.com (`microsoft-personal`)],
  [`./suzatary auth <address>` for reading, then again with `--scope smtp` for
    sending: two consents, so a leaked read token cannot send],
  [Proton (`proton`)],
  [Proton Bridge's own login, its password set as `PROTON_BRIDGE_PASSWORD`, and
    a `proton` group in `mbsyncrc`],
)

More accounts are more `[[mail.accounts]]` entries in `instance.toml`.

== A Google account

A Google account can instead be connected from the app, by one sign-in with
Google that grants its mail and its calendars (view and edit). Its
entry is `sync = "google"`: the box keeps the grant in the secret store, hands
lieer a token good for an hour before each sync (never the grant itself), and
reads the calendars through Google's
Calendar API, so no iCal link is needed for them. An event is written to the
calendar only when you tap its card.

Google lets an app sign in only through a client its owner made, so you make
one, once, for every Google account the box will hold. The Inbox asks for it
on a card that lists these steps and the exact address to paste:

+ Open the Google Cloud console (the card's button). If it asks, create a
  project, then press *Get started*: name the app, give your address, choose
  *External*, and press *Create*.
+ Under *Audience*, press *Publish app*, so the app is *In production*.
  Gmail's access is restricted, so Google shows one warning when you sign in
  ("Google hasn't verified this app"): choose *Advanced*, then *Go to the
  app*. If the app stays in *Testing*, Google ends each sign-in after 7 days,
  and the box asks you to sign in again every week.
+ Under *APIs & Services*, *Library*, enable the Gmail API and the Google
  Calendar API.
+ Under *Clients*, press *Create client* and choose the type *Web
  application*.
+ Under *Authorized redirect URIs*, add exactly your app's address followed
  by `/api/oauth/google/callback` (the card shows it whole), and press
  *Create*.
+ In the dialog that opens, press *Download JSON*, and paste the whole file
  into the card's field.

The box keeps the client only when the file is a web client listing that
exact address. The account's own card then opens Google's consent page; Google
sends you back to the app, and the box keeps what it granted. If you untick
the calendars on Google's page, the mail is still kept, and a card of its own
asks Google for the calendars alone. A sign-in Google
later refuses (revoked, or ended by a client still in Testing) puts the same
card back in the Inbox.

== Running it on NixOS

Your system's configuration takes the app's NixOS module from the app's own
flake (`nixosModules.default` of `git+<app>`, where `<app>` is the https URL you fetch the app from),
and points it at your checkout; which app the units run is your repository's
lock, never the configuration's:

```nix
services.suzatary = {
  enable = true;
  user = "ada";
  userRepo = "/home/ada/secretary";
  web.tlsDomain = "ada-box.tailnet.ts.net";
};
```

`web.tlsDomain` is the box's MagicDNS name, the one you gave `init`: the
module mints a certificate for it with `tailscale cert` and serves the app at
`https://` that name. Turn on HTTPS certificates in your tailnet's admin
console first. Without it the app is plain http, which a browser treats as
insecure: the app cannot verify your todos' history, and a phone cannot
install it.

After `nixos-rebuild switch`, run `systemctl --user start suzatary-roots`
once. It builds the app your lock names and links it where every unit runs it
from. From then on the units sync mail every five minutes, let the triage
agent propose, let the secretary review, serve the app on your tailnet, and
commit and push what the box wrote to your repository.

A unit file under `~/.config/systemd/user/` takes precedence over the ones the
module writes, so check what really runs:

```console
$ systemctl --user show -p FragmentPath suzatary-triage.timer
```

It must name a path under `/etc/systemd/user`.

#shot("list-desktop.png", [The app at desktop width: the list beside a todo's reading.])

== Deploying and rolling back

A deploy is a commit to your repository that moves its lock. From any
checkout of the app:

```console
$ scripts/deploy.sh <full commit id on master>
```

It pins the lock, commits "Deploy <rev>", waits for the units to build that
app, restarts the web only if the build succeeded, waits for it to answer,
and pushes. On any failure it reverts its own commit, so every unit keeps
running what it ran. A rollback is a deploy of an earlier commit.

== What only you can write

Three parts of your persona are personal by nature, so they never go in
`instance.toml`: they live in `local/persona.toml`, in the box-local tier
`local/`, which `init` begins with a `.gitignore` of its own so no push carries
it and the vault does. `anchors` is what a bare percentage means in stakes (the
triage agent and the secretary price every todo against them); `email_voice`,
how your mail reads; and `news_filter`, what a briefing should mention.
`local/triage-notes.md` holds your own triage rules: which senders never make a
todo, which chain a kind of mail belongs to.

#shot("inbox-desktop.png", [The Inbox: a batch the secretary held, with its reasons.])

== Notifications

Optional, and off until `instance.toml` says otherwise. Add

```toml
[notify]
channels = ["ntfy"]
```

set `NTFY_TOPIC` to a random topic, and subscribe the ntfy app on your phone
to it. Every message is opaque: a title, a count and a link into the app.

== What stays on the box

Your repository's `.gitignore` keeps out what must not be in its history:
mail (a cache of the servers), the memory's words (forgetting a fact deletes
them), rendered pages and the plugins' caches (rebuilt by syncing), and every
lock. The box commits everything else it writes, and its push refuses any
file outside that set.

#shot("todo-phone.png", [One todo: its number, its detail, and why the number is what it is.])

== Not yet solved

What `init` and the module do not do for you yet, said plainly:

- *The mail index is yours to make.* Mail is searched through notmuch, and
  `init` names no notmuch configuration, so the app and the agents refuse
  every search until you write one and set `notmuch_config` under `[mail]`
  in `instance.toml`. A Proton account also needs its `proton` group in
  `mbsyncrc`.
- *The module assumes Proton.* It always runs the Proton Bridge unit and
  the daily encrypted vault snapshot (seven kept), which uploads to Proton Drive and
  needs the Drive CLI signed in and a vault key of your own. Without a
  Proton account both units fail on every start and nothing else depends on
  them; there is no option to turn them off yet.
- *The secretary's manuals price against your anchors*, which `init` leaves
  empty: write them before you trust a number.
- The Android portal is built by Nix and signed by you, with your own
  keystore and, for push, your own Firebase project.
- One person per deployment, reached only over a tailnet.
- The app's repository is private until its owner publishes it.
