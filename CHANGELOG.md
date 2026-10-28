# Changelog

All notable changes to Suzatary. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The version is a
reading of the project's own chain of releases (CLAUDE.md, "Versioning"):
MAJOR the last completed release's, MINOR the completed items of the next,
PATCH the releases since MINOR moved.

## [Unreleased]

## [2.1.31] - 2026-10-28

The first public release. What exists:

### Added

- **Todos priced by fulfillment.** Each todo's history is one fulfillment
  function of time, built from its spec revisions, completions and
  reopenings; the list is sorted by its value now, lowest (most urgent)
  first. Specs are terms of FPL, priced only by the
  [Prodrome](https://github.com/fulfilium/prodrome)'s evaluator.
- **Stores that are replicas.** The todo chains, the Inbox, the
  conversations, the secretary's memory and the recorded decisions are each
  a Prodrome at a schema of its own: content-addressed objects in a DAG,
  merged by union, with concurrent writes to one register shown as a
  conflict rather than resolved by a clock.
- **The triage agent.** Every five minutes, new mail from Gmail, Microsoft
  and Proton accounts is read by an agent behind a deny-by-default fence,
  holding a capability record with no outward effect, its writes cards in the
  owner's Inbox.
- **The secretary.** A conversation in the app, with a memory whose words
  can be forgotten while its history stays verifiable. It reviews the
  agent's cards, sweeps the open list for evidence that a todo is done,
  and proposes emails, sign-ins, secrets and coding sessions, each performed
  only by the owner's tap.
- **The app.** A TypeScript web app with no framework, served over a
  tailnet and gated by its logins: the list and its tree of subtodos, a todo's
  reading and its thirty-day curve, the Inbox, the chat, Settings, and the
  calendar. Each tab holds a replica and folds it with the same core,
  compiled to WebAssembly. Text is Typst, edited with highlighting, completion
  and a live preview, and compiled in a closed world.
- **The model.** Every job runs a Claude model through the `claude` CLI on
  the person's Claude subscription. The model is a port in the core, so a
  deployment can provide another implementation of it.
- **Plugins.** WebAssembly components whose authority is their imports; the
  first reads calendars from their iCal links.
- **The site.** A static site of the attestable history, built from the
  chain.
- **Deployment.** A template a user's private repository starts from; one
  `instance.toml`, written by `suzatary init`; a NixOS module with its user
  units; an Android portal onto the app; and a guide rendered by Typst
  (`nix build .#docs`).
