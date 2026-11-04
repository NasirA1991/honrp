# AUR package: `honrp-bin`

`honrp-bin` installs the static release binary from GitHub Releases. The Lua
bridge is embedded in the binary, so the package only ships `honrp`; each user
runs `honrp install` once to write the bridge into their HoN Proton prefix and
enable the systemd **user** service.

## Build locally

```bash
makepkg -si
honrp install
```

## Publish to the AUR

You need an AUR account and an SSH key uploaded to
<https://aur.archlinux.org> (My Account → SSH Public Key).

```bash
# first-time import (the repo is created on the AUR side when you submit)
git clone ssh://aur@aur.archlinux.org/honrp-bin.git
cd honrp-bin
cp /path/to/honrp/packaging/aur/PKGBUILD .
cp /path/to/honrp/packaging/aur/.SRCINFO .
git add PKGBUILD .SRCINFO
git commit -m "Initial import: honrp-bin 0.2.2"
git push
```

Alternatively use **Submit Package** on the AUR website and paste the
`PKGBUILD`/`.SRCINFO`.

## Update for a new release

1. bump `pkgver` in `PKGBUILD`,
2. update `sha256sums` (binary + `LICENSE` + `config.example.json` at the tag),
3. `makepkg --printsrcinfo > .SRCINFO`,
4. commit/push.

The package name is `honrp-bin` (AUR convention for release binaries). If you
prefer a longer name, use e.g. `heroes-of-newerth-rpc-bin` consistently.
