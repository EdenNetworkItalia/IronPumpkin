# IronPumpkin

IronPumpkin is a Pumpkin-based Minecraft server with NeoForge API compatibility, maintained by
EdenNetwork Italia. Upstream is `Pumpkin-MC/Pumpkin` (git remote `upstream`); this repo is `origin`.
`AGENTS.md` (upstream rules) still applies to all code changes, with one difference: PRs are opened
on this repo, not upstream.

## Orchestration

The session agent plans, creates issues, keeps the issues and the status issue updated, and
reviews delegated results. It works inline by default and delegates only when one holds:

- a lookup or sweep would read more than ~30k tokens and its answer is short: `retriever`;
- large work splits into independent chunks that run in parallel;
- a review must not be anchored on the session's own reading.

A task that fits in ~10 tool calls, one verification run included, stays inline. Design choices,
ambiguous scope and subtle bugs stay with the session model. Every Agent call passes `model` and
`effort`; pick the cheapest pair that fits:

| Task | `model` | `effort` |
|---|---|---|
| Lookups and read-only sweeps (`retriever`) | `haiku` | `medium` |
| Mechanical edits from an exact spec | `haiku` | `low` |
| Run commands or tests and report | `haiku` | `medium` |
| Docs, config or research that needs judgment, too large for inline | `sonnet` | `medium` |
| Implementation from a clear spec with a test, too large for inline | `sonnet` | `medium` |
| Longer or harder implementation, debugging from a reproducer | `sonnet` | `high` |
| Independent review of a non-trivial or risky change set | `opus` | `high` |
| Hidden edge cases: protocol and wire format, chunk and NBT formats, codegen, event dispatch, locks and concurrency, performance | `opus` | `xhigh` |

- Close open choices in the spec before a `high` or `xhigh` agent runs. Tell it to report an
  ambiguous requirement instead of picking a reading. Do not push `sonnet` above `high`.
- When an agent fails, find the cause before the retry: a vague prompt needs a better prompt; a
  skipped file or an early stop needs one more `effort` level; a wrong result with the right
  context needs the next model. One step per retry.
- One issue per delegated agent. The brief (~300 tokens) carries the issue number, the acceptance
  criteria, `path:line` pointers to what the session already read, the verification commands and
  the report format. The report starts with `done`, `blocked` or `question` and gives conclusions,
  not file dumps.
- Every brief states that the agent must not spawn sub-agents (no Agent tool, no Workflow) and
  must read inline. Nested agents multiply token cost and reload the whole rule set.
- Independent tasks run in parallel in one message.

## Issues and specs

Work is tracked as GitHub issues on `EdenNetworkItalia/IronPumpkin`, one issue per task.

- Run `gh` with the sandbox disabled: the keyring token is blocked inside it. Always pass
  `-R EdenNetworkItalia/IronPumpkin`: this checkout has no gh default repo and also has the
  `upstream` remote. Never create, edit or comment on anything in `Pumpkin-MC/Pumpkin`.
- Every issue has the milestone of its phase and labels: `phase:*`, `model:opus` or
  `model:sonnet`, `bug` and `upstream` for Pumpkin bugs, `later` for parked work,
  `needs-owner` for work that waits on a play test or an owner decision.
- An issue body has the description, `## Acceptance criteria` as a checklist, `## Notes`,
  `## Commits` (hash and subject) and "Depends on #N (title)" lines for its dependencies. Done
  issues are closed with `gh issue close --reason completed`.
- Taking an issue means tracking it. The agent (or the session on its behalf) sets the
  `in-progress` label and the assignee and comments "Started by <model> agent" before any work;
  posts progress when a step lands or a blocker appears; posts the evidence per acceptance
  criterion at the end; the session closes it with the commit hashes and removes the label.
  An issue is never left half-tracked: either it carries `in-progress` and a live owner, or it
  is open with no label and free, or it is closed.
- Never take an issue that has `in-progress` or an assignee, even if it looks stale: a colleague
  or another harness may be working on it from another machine. If it looks abandoned (no
  comment or commit for days), ask the owner in the status issue and leave it.
- The pinned "Orchestration status" issue is the single status document. Update its body
  (`gh issue edit <n> --body-file`) at every transition, never from a file you have not checked:
  a timed-out `gh issue view` writes an empty file and `--body-file` then wipes the issue (it
  happened once; the body came back from the edit history through the GraphQL
  `userContentEdits` field). Guard with `test -s` and a size check before every edit. Transitions: wave start, issue done, review done,
  commit, owner decision. It also holds the decisions, the roadmap and the risks.
- Specs and designs live in `openspec/`: one change per phase (`openspec/changes/<name>/`) with
  proposal.md, design.md, delta specs and tasks.md. tasks.md mirrors the phase's issues: each item
  links its issue number and is checked when the issue closes. Constraints for agents are in
  `openspec/config.yaml`.
- Run `openspec status --change <name>` and `openspec validate --all` before handover. Archive a
  change (`openspec archive <name>`) when its phase lands.

## Upstream sync

Keep IronPumpkin changes in separate crates or clearly bounded modules where possible, so that
`git merge upstream/master` stays cheap.

Merge steps for `git merge upstream/master`:

1. Once per checkout, set the merge driver that `.gitattributes` names for generated files:
   `git config merge.keep-ours.driver true`. The driver is local config, so a fresh clone needs
   this line. Without it a merge writes conflict markers into the generated files.
2. Run the merge. The files under `crates/pumpkin-data/src/generated/` and
   `crates/pumpkin-wasm-host-v0_2/src/generated_packets.rs` keep our side. The v0.1 host has no
   generator: merge `crates/pumpkin-wasm-host-v0_1/` by hand. Resolve the other conflicts as usual
   (`rerere` is on).
3. Regenerate everything from the merged `assets/` and codegen sources, then format. Codegen output
   in `pumpkin-plugin-api` is not formatted, so `cargo fmt` is part of the step:

   ```bash
   cargo run --locked -p pumpkin-codegen
   cargo run --locked -p pumpkin-codegen -- wit
   cargo fmt --all
   ```

4. Review `git diff --stat` and the diff of the regenerated files. They must differ from the merge
   result only by what upstream changed in `assets/` or in the generators. A large unrelated diff
   means a fork change in a generator was lost in the merge: fix the generator, not the output.
   Commit the result as its own commit after the merge commit.

A codegen run on a clean checkout of `master` changes nothing: the generators write NBT through
`tools/pumpkin-codegen/src/canonical_nbt.rs`, which sorts compound keys. A diff on a clean tree is a bug.

## Writing to the owner

Always write with full context. Never refer to a plan item, decision, task or option by a bare
number or letter ("#4", "option A", "phase 2", "reading B"). Name the thing and what it means, as if
the reader has no memory of this session: "the content registry task", "the choice between porting
mods written for NeoForge 21.1 (1.21.1) to 26.3 and pinning IronPumpkin to an old Pumpkin commit".
Task ids, file paths, commands and commit hashes are fine when they are the subject itself.

## Keep the issue list small

Open issues are work that starts within the current phase, nothing more. Create issues for the
phase in progress only, at most one wave ahead. Parked work carries the `later` label; remove it
when the issue enters a wave. Later phases stay in the roadmap of the pinned "Orchestration
status" issue and in their OpenSpec change until the previous phase is done. Close an issue as
soon as its work lands; an issue that stays open for weeks without work is noise, not a plan.
A vanilla-parity fix that belongs upstream closes like any other issue when it lands here; the
upstream PR is tracked by its own issue with the `upstream-pr` label and no milestone, so the
milestones stay clean. Upstream PRs must be perfect: branch on the fork `EdenNetworkItalia/Pumpkin`
from upstream master, reproduce on upstream first, pass upstream's AGENTS.md checks, cite the
decompiled vanilla reference; the owner opens the PR.

## Review of a wave

Verify once per change set, not once per agent. A small diff gets its tests run inline. When
every task of a wave is done, before the commit, one `opus` agent with `effort: high` reviews the
whole wave's diff against vanilla, NeoForge sources and AGENTS.md. It gets the spec and the diff,
not the implementer's summary, and reproduces the bug or exercises the feature. Keep each review
small: split by crate or concern when the diff exceeds roughly 800 changed lines, and brief the
reviewer with the tasks' acceptance criteria, the reference sources and the question each slice
must answer. The session fixes small findings itself; larger findings go back to the implementing
agent or into a follow-up task.

## Resuming after a context reset

The repo carries the whole state. A new session (or one after `/clear`) needs no explanation
from the owner. Resume in this order:

Run `gh` with the sandbox disabled and with `-R EdenNetworkItalia/IronPumpkin`.

1. The pinned "Orchestration status" issue: find it with
   `gh issue list -R EdenNetworkItalia/IronPumpkin --json number,title,isPinned --jq '.[] | select(.isPinned)'`,
   then `gh issue view <n> -R EdenNetworkItalia/IronPumpkin`. It says which wave is running,
   which issues are in it, what the reviewers found, what waits for the owner's decision, what
   the next wave is, and holds the decisions, the roadmap and the risks.
2. `gh issue list -R EdenNetworkItalia/IronPumpkin --milestone "<phase milestone title>"`: the
   issues of the current phase (add `--state all` for the done ones).
3. `openspec status --change <name>` for the change of the current phase, then its `design.md`
   under `openspec/changes/<name>/`: the reference material for implementers.
4. `git log --oneline upstream/master..master`: what IronPumpkin has on top of Pumpkin.
5. The hindsight memory bank has the owner's lessons; search it before asking anything.

Never ask the owner what was going on. Before delegating anything, align the issues with
reality: for each `in-progress` issue, check whether its work is in `git log`, in the working
tree, or nowhere; close what landed (with hashes), leave alone what another owner holds, and
free (remove label and assignee, comment why) only an issue whose `in-progress` was set by this
same checkout and whose agent is gone. If the status issue and the other issues disagree with
the working tree, trust the working tree and fix the issues. Then pick only free issues.

## Disk hygiene

Agents must not copy the repo or build it in a second location. Boot tests use the shared
`target/` of this checkout (`cargo run --manifest-path <repo>/Cargo.toml -p pumpkin` from a small
scratch directory). A separate build copy costs about 9 GB. Delete downloads, server installs and
captures from the scratchpad when the task that needed them is done, and say what was left behind.
