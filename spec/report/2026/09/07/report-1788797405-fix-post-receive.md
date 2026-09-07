## Report on task: Fix post-receive hook not creating /web/genbb

### Context

After `git push server main`, the working dir `/web/genbb` was not
created. Reproduced the exact failure locally (bare repo + push + the
deployed hook logic): the hook died with
`fatal: this operation must be run in a work tree`. Root cause:
`git checkout` refuses to set up a work tree when the target directory
does not exist. The push itself still succeeded (refs are updated
before `post-receive` runs), so the failure was easy to miss.

### Done

- `deploy/post-receive`: added `mkdir -p /web/genbb` before the
  checkout (the working dir is created on the first push).
- Verified locally: with the directory pre-created, both the env-var
  form and the explicit `--git-dir/--work-tree` form of the hook check
  the tree out correctly (12 entries). Also confirmed a fully
  up-to-date push does NOT fire `post-receive` (hooks only run when a
  ref actually moves).
- `docs/DEPLOY.md`: clarified the hook's first-push behavior and added
  a Troubleshooting section (check hook is executable, check the push
  moved commits, and the missing-directory error).
- `bash -n` passes; markdown within the 88-char cap.

### Spec/ADR amendments

- None (deploy tooling).

### Future-task notes

- On the server: update `/web/bare/genbb/hooks/post-receive` with the
  fixed version (`cp` the new `deploy/post-receive`, keep `chmod +x`),
  then any real push creates `/web/genbb`.

### Tooling/process

- Post-receive errors print as `remote:` lines in the push output;
  a hook that exits non-zero does not fail the push. Watch the push
  output and check the hook is executable.