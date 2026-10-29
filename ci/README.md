# Using paramgraph with a git server the office runs itself

Nothing here needs a special server: any git host works (GitLab CE, Gitea,
Forgejo). paramgraph plugs in at two places.

## On each workstation

```
git config merge.paramgraph.name "paramgraph three-way merge"
git config merge.paramgraph.driver "pg merge %O %A %B %A"
git config diff.paramgraph.textconv "pg eval"
```

and in the repository's `.gitattributes`:

```
*.pg merge=paramgraph diff=paramgraph
```

`git merge` then stops on design conflicts, and `git diff` / `git log -p`
compare evaluated models (values of rules, box counts, check results) next to
the text.

## On the server, for merge requests

A server merges with plain git when someone presses its merge button, so the
merge driver does not run there. The review runs in CI instead:
`gitlab-ci.yml` here checks every changed model, writes the design diff with
a red/green picture, and dry-runs the merge against the target branch. With
"pipelines must succeed" switched on, a failing check or a design conflict
blocks the merge. The same steps work in Forgejo or Gitea Actions.

## For an office without IT staff

The lightest self-hosted setup is Forgejo (one binary, small memory) with one
CI runner that has `pg` installed. Not packaged yet.
