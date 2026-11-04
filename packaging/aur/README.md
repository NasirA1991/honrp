# AUR: `honrp-bin`

The AUR package is **updated by GitHub Actions**, not from a local `makepkg`.
`.github/workflows/aur.yml` runs after every successful **Release** workflow,
downloads the published binary and generates `PKGBUILD` + `.SRCINFO` with the
release checksums.

## One-time setup

Add the AUR SSH private key as a repository secret (the same key you use for
`ssh aur@aur.archlinux.org`):

```bash
gh secret set AUR_SSH_KEY < ~/.ssh/aur_rawm
```

After that, every release updates `honrp-bin` automatically. To backfill a
release (or if the workflow was added after the tag):

```bash
gh workflow run aur.yml -f tag=v0.2.5
```

Without the secret the workflow still generates the files and exits cleanly;
it just skips the push.

## Install

```bash
paru -S honrp-bin     # or yay -S honrp-bin
honrp install         # as your user: Lua bridge + systemd user service
```

The package ships the static binary only; the Lua bridge is embedded and written
by `honrp install`.

`honrp install` is required after the package because pacman (and this
package's build) runs as root and cannot write to a per-user Wine prefix — the
prefix may not even exist before the first game launch. The command is the
user-scoped setup and also installs the service (systemd user unit, or an XDG
autostart entry without systemd). Artwork is optional and not included.
