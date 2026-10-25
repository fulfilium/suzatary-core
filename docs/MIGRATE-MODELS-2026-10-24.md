# Dropping the model providers from the box

This release talks to Claude alone, through the `claude` CLI, and the core names no provider.
Three things on a box may still name what it removed:

- **The instance file** may hold `[[providers]]` and `[models]`. The new binary refuses any
  top-level key it does not read, so it refuses a file that holds either.
- **The inbox** may hold cards that changed a model order, the reserve or a provider
  (`ConfigureOrder`, `ConfigureReserve`, `ConfigureProvider`), the subscription's sign-in under
  its old name (`ClaudeSignIn`, now `SignIn`), and cards that connected or removed the
  Anthropic API or the subscription (`Connect` or `Disconnect` of the templates `anthropic-api`
  and `claude-subscription`; the model's template is now `model`). The new reader leaves each
  one out, with every response resting on it.
- **The model health**, `model-health.json` beside the heartbeat (box-local, never pushed),
  may hold readings of a provider's models (`phala/…`, `anthropic/…`).

Nothing else stored names a provider: a receipt was logged, never stored, and the
conversations' words are words.

`suzatary secretary migrate-models [--dry]` (`app/cli/src/commands/migrate_models.rs`) carries
the inbox and the model health once, with the owner's yes.

## What the run does

- It reads every object file of the inbox as a literal, with the removed constructors added
  to the inbox's vocabulary, since this build's reader does not know them.
- It removes each proposal asked as one of the cards above: its asking and every response,
  children first. A change rests only on its own proposal's heads, so nothing kept rests on a
  removed object; the run refuses an inbox where something does, and removes nothing.
- A run stopped part way is finished by running it again; a second run removes nothing.
- After removing, the inbox must report no finding. The box no longer holds a head it served,
  so the inbox's epoch steps on the next answer, and every device drops its copy and takes the
  inbox again whole.
- A sign-in card it removes is filed again, under the new name, the next time no model
  answers.
- It keeps only the `claude-cli/…` readings of the model health.
- `--dry` prints the same lines and writes nothing.

## The run order

`<data>` is the data repository's checkout. `suzatary` is the new build's binary, from the
deploy's build (`SUZATARY_APP_REV=<rev> scripts/app-bin.sh`). The previous build refuses
`reserve` under `[secretary]`, and the new one refuses `[[providers]]` and `[models]`, so no
build may read the instance file between the edit in step 5 and the deploy in step 8: the
writers stop first and start again only after the deploy.

1. Prove the run on a scratch copy of `<data>`, never the live tree. In the copy's instance
   file, first point `data` at the copy and `[mail] heartbeat` at a path inside it (copy
   `model-health.json` there from beside the live heartbeat), then delete the `[[providers]]`
   tables and the `[models]` table, since `migrate-models` reads the instance file before
   anything else. With `SUZATARY_INSTANCE` at the copy's instance file, run
   `secretary migrate-models --dry`, `secretary migrate-models`, `verify`, and
   `secretary migrate-models` again. The run and `verify` must print no finding, and the
   second run must remove nothing. Read every `removed proposal` and `dropped` line.
2. The owner's yes: this is a live data migration.
3. Stop everything that reads the instance file or writes the inbox: `suzatary-web`,
   `suzatary-mail-idle`, the triage and plugin timers. Wait until the triage service is
   inactive. A tap on an old card can no longer write the file back.
4. Check that the model's credential is there, by its length alone, never its value:

   ```sh
   secretspec get CLAUDE_CODE_OAUTH_TOKEN --reason "migrate-models: present?" | wc -c
   ```

   It must print more than 1. If it does not, set it with `secretspec set` at a terminal
   before going on, since every job now asks the subscription alone.
5. Edit the live instance file: delete the `[[providers]]` tables and the `[models]` table.
   If the old `[models] reserve` was not 0.30, add `reserve = <that share>` under
   `[secretary]`; the new build reads it once, at start.
6. Run it against the live instance and keep its output:

   ```sh
   suzatary secretary migrate-models > /tmp/migrate-models.out; echo "exit $?"
   ```

   It must say `exit 0`. On any other exit, run it again.
7. Commit the instance file and the removed inbox objects by hand, in one commit under the
   push lock. `safe-push.sh` would refuse a deletion of a stored object, and would verify with
   the previous build, which refuses the new file:

   ```sh
   cd <data>
   exec 9>"$(git rev-parse --git-dir)/suzatary-push.lock"; flock 9
   git add instance.toml
   git add -A "<inbox>/objects/"
   git commit -m "instance and inbox: the model providers removed (secretary migrate-models)"
   git push || { git pull --rebase && git push; }
   exec 9>&-
   ```

8. `scripts/deploy.sh <rev>`. Its `verify` reads the instance file without the two tables and
   the inbox without the old cards, and it restarts `suzatary-web` on the new build.
9. Start `suzatary-mail-idle` and the timers again.

Once this has run on the box, a later series deletes the verb, its module, its test and this
run order.
