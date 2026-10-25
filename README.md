# Compressy

A small macOS app built with [zgui](https://github.com/zeronsh/zgui): drop a video in, crop it,
tune the settings against a measured estimate, and compress it with ffmpeg.

<p align="center">
  <img src="docs/screenshots/home.png" alt="The home screen: “make a small file” beside a little smiling pixel file, an About box and three numbered steps on the left, and a dithered Drop window on the right" width="900">
</p>

<table>
  <tr>
    <td width="50%"><img src="docs/screenshots/review.png" alt="Review: a before and after split of the frame, with quality, size, rate, sound and codec settings and a measured estimate of 18.6 MB, 63% smaller"></td>
    <td width="50%"><img src="docs/screenshots/crop.png" alt="Crop mode: a 1:1 box over the frame with dithered shades outside it, and the estimate updated to 12.3 MB"></td>
  </tr>
  <tr>
    <td align="center"><sub>Review: tune against a measured estimate, split before and after</sub></td>
    <td align="center"><sub>Crop: drag a box, or pick an aspect; the estimate follows</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/screenshots/compressing.png" alt="Compressing: the frame Atkinson-dithered to 1-bit behind a small window at 72%, with the ffmpeg log bar along the bottom"></td>
    <td width="50%"><img src="docs/screenshots/done.png" alt="Done: 50.8 MB before, 18.0 MB after, 64% smaller, with View and Reveal buttons"></td>
  </tr>
  <tr>
    <td align="center"><sub>Compressing: one ffmpeg pass, its log in a drawer</sub></td>
    <td align="center"><sub>Done: saved beside the original</sub></td>
  </tr>
</table>

## Install

```sh
brew install ffmpeg zig@0.16   # ffmpeg does the encoding; Zig builds Ghostty, the log's terminal
scripts/bundle.sh --install    # builds Compressy.app (and a .dmg in dist/), puts it in /Applications
```
