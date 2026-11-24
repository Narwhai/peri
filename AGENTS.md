# Agent notes for Peri

## Version control: this is a Jujutsu (jj) repo

This repository is primarily managed with **Jujutsu (`jj`)**. It is colocated, so a `.git` directory also exists, but use `jj` for day-to-day work.

- Check state with `jj st` and `jj log`, not `git status`.
- The working copy is itself a commit (`@`). Edits are snapshotted automatically, so there is no staging area and no `git add`.
- Describe work with `jj describe -m "..."`. Start new work with `jj new`. Use `jj commit -m "..."` to describe the current change and start a fresh one.
- Use bookmarks instead of git branches: `jj bookmark create <name>`, `jj bookmark set <name> -r <rev>`.
- For isolated parallel workspaces, use `jj workspace add <path>`, not `git worktree add`.
- Avoid mutating git commands (`git commit`, `git checkout`, `git rebase`, `git reset`). They can leave jj and git out of sync. Read-only git commands are fine.
- Push with `jj git push`.
- Keep commits small and focused, with descriptive messages.
