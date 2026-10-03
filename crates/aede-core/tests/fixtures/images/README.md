# Artwork validation fixtures

These are real images generated locally with Pillow and ImageMagick, rather than format signatures standing in for images. No test needs either tool or another external helper.

- `baseline.jpg`: 32×32 RGB gradient, JPEG quality 90, 4:2:0 sampling.
- `progressive.jpg`: the mirrored gradient, progressive JPEG quality 90.
- `cmyk.jpg` and `grayscale.jpg`: CMYK and grayscale JPEG versions of the gradient.
- `rgba.png`: one RGBA pixel with partial transparency.
- `palette.png`: the gradient reduced to eight palette entries.
- `grayscale16.png`: four grayscale pixels at 16-bit depth.
- `interlaced.png`: a 17×13 RGBA gradient encoded with Adam7, exercising all seven passes and multiple rows.
- `indexed-interlaced.png`: its 17×13 indexed counterpart, with eight palette entries and four-bit pixels.
- `animated.png`: two RGBA frames, to verify the explicit refusal of animated artwork.

The fixture generator used `Image.new`, `putdata`, `save`, `transpose`, and `convert` from Pillow. The Adam7 images were encoded with the following ImageMagick commands (run in this directory):

```sh
magick baseline.jpg -resize '17x13!' -alpha set -channel A -evaluate set 50% +channel -depth 8 -define png:color-type=6 -interlace PNG interlaced.png
magick palette.png -resize '17x13!' -colors 8 -define png:color-type=3 -define png:bit-depth=4 -interlace PNG indexed-interlaced.png
```

Every output was opened and fully decoded again with Pillow before inclusion. Corruption and size-limit tests alter copies of these images; the originals remain valid.
