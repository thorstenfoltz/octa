# `--completions`

Print a shell completion script to stdout.

```bash
octa --completions zsh
octa --completions bash
octa --completions fish
octa --completions powershell
octa --completions elvish
```

The script is generated from Octa's own argument list, so every flag `--help`
knows about is completed, including file arguments (`--schema <TAB>` completes
paths) and enum values (`--target <TAB>` lists the schema-export targets).

## Using it right now

Nothing is written to disk, so you can wire it into the shell you are sitting
in:

```bash
eval "$(octa --completions zsh)"      # zsh
eval "$(octa --completions bash)"     # bash
octa --completions fish | source      # fish
```

Put the same line in `~/.zshrc`, `~/.bashrc` or
`~/.config/fish/config.fish` to keep it.

## Installing the files

`install.sh` writes the completion files for you as part of a normal install,
under the prefix it is installing to:

| Shell | File                                                |
|-------|-----------------------------------------------------|
| bash  | `$PREFIX/share/bash-completion/completions/octa`    |
| zsh   | `$PREFIX/share/zsh/site-functions/_octa`            |
| fish  | `$PREFIX/share/fish/vendor_completions.d/octa.fish` |

A system-wide install (`sudo ./install.sh`, prefix `/usr` or `/usr/local`)
lands in the directories every shell already reads. A user-local install
(`./install.sh ~/.local`) puts them under `~/.local/share`, which bash picks up
on its own; for zsh, add the directory to your `fpath` before `compinit`:

```zsh
fpath=(~/.local/share/zsh/site-functions $fpath)
autoload -Uz compinit && compinit
```

Writing the files is best effort: a shell whose directory cannot be written is
skipped, and the rest of the install carries on. PowerShell has no standard
location on Linux, so it is generated on demand only.

## Refresh after an upgrade

The script is a snapshot of the flags at the moment it was generated. After
upgrading Octa, rerun `install.sh` (or your `eval` line) so new flags complete.
