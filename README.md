# Sony ZV-E1 Mods

Two small mods for the Sony ZV-E1 on firmware 2.02, as macOS double-click
launchers. Both mods live in the camera's memory only. Pop the battery for
ten seconds and the camera is back to stock.

Firmware 2.02 only. The mods refuse to run on anything else.

## The mods

**Card-speed bypass.** The ZV-E1 refuses to record high-bitrate modes (like
4K 120p or 4K 60p 600 Mbps) on cards whose declared speed class it does not
know, even when the card is plenty fast. This mod removes that refusal.
Recording still fails if the card cannot keep up; it just lets you try.

**Less noise reduction.** The ZV-E1 runs a temporal noise-reduction stage
that smears fine detail between frames. This mod skips it. Expect roughly
15 to 25 percent more frame-to-frame grain and a sharper picture. Not a
full NR off, just this one stage.

Open gate (3840 x 2560, the full sensor) is a separate project by
ArcaneLantern at https://github.com/ArcaneLantern/zve1-open-gate-test. Use
theirs directly.

## How to run a mod

Download the zip for the mod you want from the Releases page, unzip it, and
open the folder. Then:

1. Double-click `setup.command` the first time. It builds a small Python
   environment inside the folder. No Homebrew or Xcode project needed; the
   Command Line Tools (`xcode-select --install`) are enough. Give it a
   minute. When it prints "Done. Setup complete.", close it.

2. Put the camera's mode dial on Movie. Turn the camera off, plug it into
   the Mac, then turn the camera on.

3. Double-click the launcher for the mod:
   * `card_speed_bypass.command`
   * `less_noise_reduction.command`

4. macOS asks for your Mac password. It needs admin rights to talk to the
   camera on the USB bus.

5. The camera disappears from USB for a few seconds and comes back. The
   launcher prints "DONE." Close the window.

For the noise-reduction mod, unplug the camera, flick the Still/Movie dial
stills → movie → stills → movie, and start recording. That cycle is what
locks the new setting in.

For the card-speed mod, you can start recording right away.

## Undo

Every mod is reset by pulling the battery for ten seconds. That is always
the simplest way back.

For the noise-reduction mod there is also `undo_less_noise_reduction.command`
and `check_less_noise_reduction.command` (which just reports the current
state) so you can undo it without powering the camera off.

## Safety

Nothing in these mods writes the camera's firmware or its saved settings.
No menu item changes, no persisted flag, nothing in the backup region. A
battery pull reverts everything.

Each mod checks the bytes it is about to change against the exact stock
bytes on firmware 2.02. If they do not match it refuses and leaves the
camera alone. On any firmware other than 2.02 that check will fail, so the
mod refuses.

You run this at your own risk. See LICENSE.

## Credits

Built by itsAllexB.

Sony-PMCA-RE by ma1co (https://github.com/ma1co/Sony-PMCA-RE) is bundled
to open the camera's service mode on macOS; see its `LICENSE.txt` in the
third_party folder.

The service-mode delivery channel used here was reverse-engineered by
ArcaneLantern (Athiril) in their A7 IV and A7S III open-gate kits.
