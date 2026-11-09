# Contributing

Every commit needs a `Signed-off-by` trailer. Create it with `git commit -s`.
The email in that trailer is `git config user.email`.

Bot commits and merge commits are skipped. A `Co-authored-by` trailer for a
person is fine. An `anthropic.com` co-author, or a `Claude-Session` trailer,
fails the DCO check.

Pull request titles use a conventional type: `feat`, `fix`, `docs`, `style`,
`refactor`, `perf`, `test`, `build`, `ci`, `chore`, or `revert`. A scope is
optional.

To repair an unsigned branch:

```bash
git rebase --signoff origin/master
```
