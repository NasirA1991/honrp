# Art assets

`hon.png` (512×512), `hon-transparent.png` and `hon@1024.jpg` are generated
from the **Heroes of Newerth: Reborn** client logo
(`ui/fe3/elements/reborn_logo.tga` inside `resources0.jz`) and are used as the
Discord Rich Presence `large_image` under the asset key **`hon`**.

They are derived from HoN/Juvio artwork, © Juvio, LLC. They are included here
for personal use with the game; if you redistribute this repository publicly,
review that or replace them with your own neutral artwork. The raw 1024×1024
TGA is intentionally not committed.

Regenerate (requires Pillow and a local game install):

```bash
# extract the logo from your own install
python3 - <<'PY'
import zipfile
z = zipfile.ZipFile("/path/to/heroes of newerth/resources0.jz")
open("reborn_logo.tga", "wb").write(z.read("ui/fe3/elements/reborn_logo.tga"))
PY
python3 make_asset.py reborn_logo.tga
```

Upload `hon.png` in the Discord developer portal under
**Rich Presence → Art Assets**, asset name **`hon`**.
