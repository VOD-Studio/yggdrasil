# Issue tracker: GitHub

Issues and specs for this repo live in GitHub Issues at `VOD-Studio/yggdrasil` (the `origin` remote). Use the `gh` CLI for tracker operations.

## Conventions

- Create: `gh issue create --title "..." --body "..."`
- Read: `gh issue view <number> --comments`
- List: `gh issue list --state open`
- Comment: `gh issue comment <number> --body "..."`
- Label: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- Close: `gh issue close <number> --comment "..."`

Infer the repository from `origin` when running `gh` inside this clone.

## Pull requests as a triage surface

PRs are not treated as a request surface for triage.
