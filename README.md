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

- **Review before anything runs.** Dropping a clip (or ⌘O, or "Choose a file…") opens a review
  screen: quality, resolution, frame rate, audio and codec, plus an Advanced disclosure with a
  quality fine-tune and an option to keep metadata. Compressy encodes three 2-second samples with
  exactly those settings (the whole clip if it's short) to estimate the final size, the encode
  time and the VMAF score, and shows a frame before and after on a split you scrub with the
  pointer.
- **Crop.** Crop mode draws a box over the frame: drag it, its edges or corners, or draw a new one;
  Free, 16:9, 9:16, 1:1 or 4:5. The crop flows into the estimate, the encode and the AI prompt.
- **Compress.** One ffmpeg pass (HEVC/x265 by default; H.264 or AV1 if you choose). The frame
  stays on stage with live progress read from ffmpeg's `-progress` file, then the result. ffmpeg's
  log runs in an embedded Ghostty terminal in a drawer along the bottom.
- **Copy prompt for AI.** A small action under Compress (also in the Clip menu) copies a brief
  for an AI with a shell (Claude Code, Codex, …): the clip, your settings and crop, the measured
  baseline to beat, and how to search for a smaller encode. Compressy itself runs no AI.
- **No ffmpeg?** A footnote on the home screen shows how to install it (Homebrew, or brew.sh
  first) and goes away once it's there.

Output goes beside the original as `name.compressed.mp4`; the original is never touched.

The look is small-file and calm, after [solweb](https://solweb.sina.town/) and
[smallfile](https://smallfile.ca/): off-white paper, ink and one orange accent; Geist Mono captions
with Helvetica Neue for the prose; 1-bit everything. Fills are ordered (8×8 Bayer) dither from a
small fill shader (`src/ui/dither.rs`, adapted from zgui's composer example), so there is no grey,
only density: the drop window's desk thickens as a file comes over, crop shades are 62% ink, meters
fill with a checker. Frames are little OS windows with striped title bars and dithered drop
shadows, the title is a 5×7 pixel wordmark, a little pixel file smiles (and blinks) beside "make a
small file", and the frame on stage during an encode is
Atkinson-dithered. Tokens live in `src/ui/theme.rs`, motion curves in `src/ui/motion.rs`;
segmented controls are accessible (one Tab stop, arrow keys).

It is native where macOS is: a real menu bar (File, Edit, Clip, Quality, View, with shortcuts like
⌘O, ⌘↩, ⌘1/⌘2 and ⌘L, enabled and checked as the app changes), the open panel, an AppKit alert
before stopping an encode under way, AppKit context menus on the plate, tooltips, and zgui's own
scroll view, disclosure, slider, checkbox and progress bar. The window remembers its frame, and its
title follows the clip.

## Build

Needs Rust and Zig 0.16 (`brew install zig@0.16`), which builds Ghostty on the first build.

```sh
cargo run --release                 # or: target/release/compressy path/to/clip.mov
COMPRESSY_HIDE=ffmpeg,brew cargo run --release     # preview the setup footnote
COMPRESSY_PREVIEW_HOVER=1 cargo run --release      # the drop zone with a file over it
COMPRESSY_PREVIEW_CROP=1:1 target/release/compressy clip.mov    # review, in crop mode
COMPRESSY_PREVIEW_COMPRESS=1 target/release/compressy clip.mov  # presses Compress
COMPRESSY_PREVIEW_ADVANCED=1 target/release/compressy clip.mov  # the Advanced disclosure open
cargo test --release
```

Fonts: Geist and Geist Mono (SIL OFL); icons from zgui's examples (MIT). See `assets/README.md`.
