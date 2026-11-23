//! The pushes to the public repository: the rows of its master (`train publish`) and a gate
//! branch (`train push-gate`), each through [`publishable`].

use crate::admitted::{Admitted, Terminal};
use crate::core_rows::{CORE_MASTER, Core};
use crate::fresh::{Compared, Fresh, Push};
use crate::gate_branch::GateBranch;
use crate::instant::Instant;
use crate::known::Known;
use crate::matcher::Matcher;
use crate::oid::Oid;
use crate::plan::{Plan, Source};
use crate::public_tree::{PublicTree, Script};
use crate::publishable::{Publishable, publishable};
use crate::refusal::Refusal;
use crate::repo_path::RepoPath;
use crate::{Error, git, process};

/// The publications' author and committer: no person's name rides a publication.
pub(crate) const AUTHOR: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "Suzatary"),
    ("GIT_AUTHOR_EMAIL", "publish@suzatary.invalid"),
    ("GIT_COMMITTER_NAME", "Suzatary"),
    ("GIT_COMMITTER_EMAIL", "publish@suzatary.invalid"),
];

#[derive(Clone, Debug)]
pub struct Guard {
    pub script: Script,
    pub matcher: Matcher,
}

/// `train publish`: core's master gains the public tree of each landed commit up to `rev`
/// (origin's master), oldest first, until one cannot be published. The status: 0 published or
/// already, 1 a leak or a held path stopped it.
pub fn publish(
    guard: &Guard,
    rev: Option<&str>,
    skips: &[String],
    admits: Vec<RepoPath>,
) -> Result<u8, Error> {
    let admits = match (admits.first(), Terminal::stdin()) {
        (None, _) => Vec::new(),
        (Some(_), Some(terminal)) => admits
            .into_iter()
            .map(|path| Admitted::at_terminal(path, &terminal))
            .collect(),
        (Some(path), None) => return Err(Refusal::AdmitAway(path.clone()).into()),
    };
    if !git::holds(&["remote", "get-url", "core"])? {
        return Err(Refusal::NoCore.into());
    }
    unpublished(
        git::act(&["fetch", "--quiet", "origin", "master"])?,
        "could not fetch origin's master",
    )?;
    let named = |word: &str, refused: fn(String) -> Refusal| {
        git::commit_if(word)?.ok_or_else(|| Error::from(refused(word.to_owned())))
    };
    let rev = named(rev.unwrap_or("origin/master"), Refusal::RevUnknown)?;
    let skips = skips
        .iter()
        .map(|skip| named(skip, Refusal::SkipUnknown))
        .collect::<Result<Vec<_>, _>>()?;
    let chain = git::first_parents("origin/master")?;
    let core = fetch_core()?;
    let plan = match Plan::of(&chain, core.rows(), &rev, &skips)? {
        Plan::Published(row) => {
            eprintln!(
                "train: core already publishes {}, as {}",
                rev.short(),
                row.short()
            );
            return Ok(0);
        }
        Plan::Rows(plan) => plan,
    };
    for skip in &skips {
        guard.script.check(skip, &guard.matcher).skip(skip)?;
    }
    let steps = listed(guard, &core, plan)?;
    if let Some(stray) = admits.iter().find(|admit| {
        !steps
            .iter()
            .any(|(_, compared)| compared.fresh.iter().any(|f| f.path() == admit.path()))
    }) {
        return Err(Refusal::AdmitNotHeld(stray.path().clone()).into());
    }
    let mut parent = core.tip().cloned();
    for (step, compared) in &steps {
        let (source, skipped) = match step {
            Source::Skip(skip) => {
                eprintln!(
                    "train: skipping {}, as asked: the next row names it",
                    skip.short()
                );
                continue;
            }
            Source::Row { source, skipped } => (source, skipped),
        };
        let checked = guard.script.check(source, &guard.matcher);
        let tree = match publishable(checked, Push::Row, &compared.fresh, &admits) {
            Publishable::Publishable(tree) => tree,
            Publishable::Leak => {
                eprintln!(
                    "train: the public tree of {} did not pass its leak guard (the files and \
                     lines above). Nothing published from it on. If a term added to the lists \
                     since it landed matches it, it can never pass: scripts/train.sh publish \
                     --skip {source}",
                    source.short()
                );
                return Ok(1);
            }
            Publishable::CouldNotRun(why) => {
                return Err(Error::Unrun(format!("{why}. Nothing published.")));
            }
            Publishable::Held(_) => {
                held(source, &steps, &admits);
                return Ok(1);
            }
        };
        say_moved(compared, source);
        let row = push_row(&tree, parent.as_ref(), skipped, compared)?;
        parent = Some(row);
    }
    Ok(0)
}

/// Each step of `plan` beside every earlier row and step.
fn listed(guard: &Guard, core: &Core, plan: Vec<Source>) -> Result<Vec<(Source, Compared)>, Error> {
    let mut known = Known::read(core, &guard.matcher)?;
    let mut steps = Vec::new();
    for step in plan {
        let compared = match &step {
            Source::Skip(_) => Compared::default(),
            Source::Row { source, .. } => {
                let written = guard.script.write(source).map_err(|why| {
                    Error::Unrun(format!("{why}, to list the paths it would hold"))
                })?;
                known.fresh(&written, &guard.matcher)?
            }
        };
        steps.push((step, compared));
    }
    Ok(steps)
}

/// One row: `tree` committed on `parent`, dated as its source, naming its skips, admissions and
/// moves, pushed to core's master.
fn push_row(
    tree: &PublicTree,
    parent: Option<&Oid>,
    skipped: &[Oid],
    compared: &Compared,
) -> Result<Oid, Error> {
    let source = tree.source();
    // One paragraph, so git reads every line as a trailer.
    let mut trailers = vec![format!("Source: {source}")];
    trailers.extend(skipped.iter().map(|skip| format!("Skipped: {skip}")));
    let mut admitted: Vec<&RepoPath> = compared.fresh.iter().map(Fresh::path).collect();
    admitted.sort();
    admitted.dedup();
    trailers.extend(admitted.iter().map(|path| format!("Admitted: {path}")));
    trailers.extend(compared.moved.iter().map(|moved| format!("Moved: {moved}")));
    let when = process::out(&mut git::cmd(&[
        "log",
        "-1",
        "--format=%cI",
        &source.to_string(),
    ]))?;
    let row = commit_tree(
        tree,
        parent,
        &when,
        &format!("Publish {}", source.short()),
        &trailers.join("\n"),
    )?;
    unpublished(
        git::act(&[
            "push",
            "--quiet",
            "core",
            &format!("{row}:refs/heads/master"),
        ])?,
        &format!(
            "core refused master at {} (did it move?); scripts/train.sh publish again",
            row.short()
        ),
    )?;
    process::out(&mut git::cmd(&[
        "update-ref",
        CORE_MASTER,
        &row.to_string(),
    ]))?;
    println!("published: core master is {row} (Source: {source})");
    Ok(row)
}

/// Each admitted file `compared` moved, one line each, for the person reading the log.
fn say_moved(compared: &Compared, source: &Oid) {
    for moved in &compared.moved {
        eprintln!("moved: {moved} (in {})", source.short());
    }
}

/// Every step's held paths, so one look admits them all, and the command that admits them; beside
/// them, the moves of the steps not yet published.
fn held(source: &Oid, steps: &[(Source, Compared)], admits: &[Admitted]) {
    let unpublished = steps.iter().skip_while(|(step, _)| match step {
        Source::Row { source: at, .. } => at != source,
        Source::Skip(_) => true,
    });
    for (step, compared) in unpublished {
        if let Source::Row { source, .. } = step {
            say_moved(compared, source);
        }
    }
    let every: Vec<(&Oid, RepoPath)> = steps
        .iter()
        .filter_map(|(step, compared)| match step {
            Source::Row { source, .. } => Some((source, &compared.fresh)),
            Source::Skip(_) => None,
        })
        .flat_map(|(at, fresh)| {
            Push::Row
                .held(fresh, admits)
                .into_iter()
                .map(move |path| (at, path))
        })
        .collect();
    for (at, path) in &every {
        eprintln!("held: {path} (in {})", at.short());
    }
    let words: Vec<String> = every
        .iter()
        .map(|(_, path)| quoted(path.as_str()))
        .collect();
    eprintln!(
        "train: the public tree of {} holds a path no publication has held; every row's are \
         above. Nothing published from it on. Look at each, then at a terminal: scripts/train.sh \
         publish --admit {}",
        source.short(),
        words.join(" --admit ")
    );
}

/// The verdict on pushing `commit`'s public tree to a gate branch, and core as it was read.
pub fn gate_check(guard: &Guard, commit: &str) -> Result<(Publishable<PublicTree>, Core), Error> {
    let commit = git::commit(commit)?;
    let core = fetch_core()?;
    let mut known = Known::read(&core, &guard.matcher)?;
    let written = match guard.script.write(&commit) {
        Ok(written) => written,
        Err(why) => return Ok((Publishable::CouldNotRun(why), core)),
    };
    let compared = known.fresh(&written, &guard.matcher)?;
    let checked = guard.script.guard(written, &guard.matcher);
    let verdict = publishable(checked, Push::GateBranch, &compared.fresh, &[]);
    say_moved(&compared, &commit);
    if let Publishable::Held(held) = &verdict {
        for path in held {
            eprintln!("held: {path} (in {})", commit.short());
        }
    }
    eprintln!("train: {}", said(&verdict, &commit));
    Ok((verdict, core))
}

/// `train push-gate`: `commit`'s public tree, if publishable, committed on core's master and
/// pushed to `branch` alone; the pushed commit on stdout. The status is [`Publishable::status`].
pub fn push_gate(
    guard: &Guard,
    commit: &str,
    branch: &GateBranch,
    subject: &str,
    trailer: &str,
) -> Result<u8, Error> {
    let (verdict, core) = gate_check(guard, commit)?;
    let Publishable::Publishable(tree) = verdict else {
        return Ok(verdict.status());
    };
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let (now, stamp) = gate_stamp(since);
    let pushed = commit_tree(
        &tree,
        core.tip(),
        &now.to_string(),
        &format!("{subject}, {stamp}"),
        trailer,
    )?;
    if !git::act(&[
        "push",
        "--quiet",
        "core",
        &format!("+{pushed}:refs/heads/{branch}"),
    ])? {
        return Err(Error::Unrun(format!(
            "core refused {branch}, so it says nothing of {}",
            tree.source().short()
        )));
    }
    println!("{pushed}");
    Ok(0)
}

/// A gate commit's date and its stamp to the nanosecond, so each gate commit, and its run, has a
/// name of its own: two pushes at different instants never make the same commit.
#[must_use]
pub fn gate_stamp(since: std::time::Duration) -> (Instant, String) {
    let now = Instant(since.as_secs());
    let stamp = format!(
        "{}.{:09}Z",
        now.to_string().trim_end_matches('Z'),
        since.subsec_nanos()
    );
    (now, stamp)
}

fn said<T>(verdict: &Publishable<T>, commit: &Oid) -> String {
    let short = commit.short();
    match verdict {
        Publishable::Publishable(_) => {
            format!("the public tree of {short} may go to a gate branch")
        }
        Publishable::Leak => format!(
            "the public tree of {short} did not pass its leak guard (the files and lines \
             above), so nothing of it leaves the box"
        ),
        Publishable::CouldNotRun(why) => format!("{why}, so nothing of {short} leaves the box"),
        Publishable::Held(_) => format!(
            "the public tree of {short} holds an admission no row of core's master holds \
             (above), so nothing of it leaves the box; gate it on the box: scripts/train.sh gate"
        ),
    }
}

/// Core's master fetched into [`CORE_MASTER`], or the local name dropped where core has none.
fn fetch_core() -> Result<Core, Error> {
    let mut heads = git::cmd(&["ls-remote", "--heads", "core", "refs/heads/master"]);
    let heads = process::out(&mut heads)
        .map_err(|e| Error::Unrun(format!("could not reach core ({e}). Nothing published.")))?;
    if heads.is_empty() {
        let _ = git::holds(&["update-ref", "-d", CORE_MASTER]);
        return Ok(Core::Unborn);
    }
    unpublished(
        git::act(&[
            "fetch",
            "--quiet",
            "core",
            &format!("+refs/heads/master:{CORE_MASTER}"),
        ])?,
        "could not fetch core's master",
    )?;
    Core::read(git::commit(CORE_MASTER)?)
}

fn unpublished(done: bool, said: &str) -> Result<(), Error> {
    if done {
        Ok(())
    } else {
        Err(Error::Unrun(format!("{said}. Nothing published.")))
    }
}

/// The commit of `tree` on `parent`; a commit step that names no commit stops here, before a push
/// of an empty name could delete core's master.
fn commit_tree(
    tree: &PublicTree,
    parent: Option<&Oid>,
    when: &str,
    subject: &str,
    trailers: &str,
) -> Result<Oid, Error> {
    let tree_id = tree.write_tree()?;
    let mut cmd = git::cmd(&["commit-tree", &tree_id.to_string()]);
    if let Some(parent) = parent {
        cmd.args(["-p", &parent.to_string()]);
    }
    cmd.args(["-m", subject, "-m", trailers])
        .envs(AUTHOR)
        .env("GIT_AUTHOR_DATE", when)
        .env("GIT_COMMITTER_DATE", when);
    let said = process::out(&mut cmd)?;
    Oid::parse(&said).map_err(|_| {
        Error::Unrun(format!(
            "committing the public tree of {} named no commit ({said:?}), and pushing it would \
             delete core's master. Nothing published.",
            tree.source().short()
        ))
    })
}

/// `path` as a shell word: itself where it holds only safe characters, else single-quoted.
fn quoted(path: &str) -> String {
    if path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "_./-+=:,@%".contains(c))
    {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}
