# skills/

Claude Code / Claude.ai skills that ship with `source-map-php`.

This directory is the **publishable copy** of each skill. The working copy lives under `~/.claude/skills/<skill-name>/` on the maintainer's machine; `scripts/sync_to_repo.sh` inside the skill mirrors that copy back into this directory before each release.

## Layout

```
skills/
└── source-map-php/
    ├── SKILL.md
    ├── references/
    │   ├── commands.md
    │   └── troubleshooting.md
    └── scripts/
        ├── env_probe.sh
        └── sync_to_repo.sh
```

## Installing locally

From a checkout:

```bash
# Copy or symlink the skill into your Claude Code skills directory:
cp -R skills/source-map-php ~/.claude/skills/
# or
ln -s "$PWD/skills/source-map-php" ~/.claude/skills/source-map-php
```

Claude Code picks up new skills on its next session start.

## Publishing

When publishing to a skill registry (skill.sh, or any other), the contents of `skills/source-map-php/` are the artifact. Commit any changes here, tag a release, and push to wherever the registry pulls from.

Workflow for the maintainer:

1. Edit the installed copy at `~/.claude/skills/source-map-php/` and iterate in real sessions.
2. Run `~/.claude/skills/source-map-php/scripts/sync_to_repo.sh` to mirror into this repo.
3. `git add skills/` and commit.
4. Publish the tagged commit / directory through whichever channel the registry expects.

## Contributing

If you're opening a PR that changes the skill, edit the files in this directory directly — the sync script is one-way from `~/.claude/skills/` into the repo, so PR edits won't be clobbered by local iteration as long as the maintainer pulls before running sync.
