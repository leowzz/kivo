# Runner Gallery artwork

These animation frames come from [Runner Gallery](https://github.com/runcat-dev/RunnerGallery),
maintained by Takuto Nakamura (Kyome22), and are licensed under Apache-2.0.
The original license is included in [LICENSE](LICENSE). Each selected runner's
author and source name are recorded in [manifest.json](manifest.json); all ten
selected runners are credited upstream to Takuto Nakamura (Kyome22).

Artwork © Takuto Nakamura (Kyome22).

Kivo modifications: sort the original keyframes numerically, apply a stable
crop across each complete cycle, resize to a 96×64 animation area, and convert
the alpha mask to white monochrome pixels. The generated bitmap arrays are in
`lib/gpio_trigger/src/RunCatSprites.h`. Original PNGs are retained here so the
conversion is reproducible without downloading assets during firmware builds.

Regenerate with:

```sh
uv run --with pillow python scripts/generate_runcat_sprites.py
```
