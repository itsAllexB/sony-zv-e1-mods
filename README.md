<div align="center">

# Sony ZV-E1 Mods

**Small mods for the Sony ZV-E1, plus an app that fixes open gate clips.**

![Camera](https://img.shields.io/badge/camera-Sony%20ZV--E1-black)
![Firmware](https://img.shields.io/badge/firmware-2.02-blue)
![Platform](https://img.shields.io/badge/platform-macOS-lightgrey)
![License](https://img.shields.io/badge/license-MIT-green)

[Mods](#the-mods) · [Open gate](#open-gate) · [Open Gate Fix app](#open-gate-fix-app) · [FAQ](#faq)

</div>

---

## What's here

| | What it is | Runs on |
|---|---|---|
| [**The mods**](#the-mods) | Two camera mods made here: card-speed bypass and less noise reduction. | macOS |
| [**Open gate**](#open-gate) | Full-sensor 3:2 recording. A separate project by ArcaneLantern; this page only points to it. | See his repo |
| [**Open Gate Fix app**](#open-gate-fix-app) | Made here. Fixes open gate clips so editors open them at the right size. | macOS, Windows, Linux |

## The mods

> [!NOTE]
> Both mods only last until the camera powers off fully. Pull the battery for
> a couple of seconds and the camera is exactly back to stock. Nothing is
> saved to the camera's firmware or settings.

| Mod | What it does | Undo | Download |
|---|---|---|---|
| **Card-speed bypass** | Lets you record high-bitrate modes like **4K 120p** and **4K 60p 600 Mbps** on fast cards the camera doesn't recognise. | Battery pull | [**Download zip**](https://github.com/itsAllexB/sony-zv-e1-mods/releases/download/v0.1.0/card-speed-bypass-macos-v0.1.0.zip) |
| **Less noise reduction** | Turns off one of the camera's noise-reduction stages, so fine detail stops getting smoothed away between frames. Adds a bit more natural grain. More visible at high ISO values. | Battery pull | [**Download zip**](https://github.com/itsAllexB/sony-zv-e1-mods/releases/download/v0.1.0/less-noise-reduction-macos-v0.1.0.zip) |

Each mod is its own download. Grab only the one you want.

### What you need

- A Sony **ZV-E1** on firmware **2.02** (check under *Setup → Version*)
- A Mac with Python 3.9 or newer
  (run `xcode-select --install` once if you don't have it)
- A USB-C **data** cable, plugged straight into the Mac
- A battery that's at least half charged

### How to run

**1. Download and set up** *(first time only)*

Download a zip from [Releases](https://github.com/itsAllexB/sony-zv-e1-mods/releases),
unzip it, open the `macos` folder and double-click **`setup.command`**.
Wait for *"Done. Setup complete."*

**2. Connect the camera**

Turn the camera off, set it to **Movie**, plug it into the Mac, then turn
it on.

**3. Run the mod**

Double-click the mod's launcher and type your Mac password when asked.

| Mod | Launcher |
|---|---|
| Card-speed bypass | `card_speed_bypass.command` |
| Less noise reduction | `less_noise_reduction.command` |

The camera drops off USB for a few seconds and comes back. Wait for
**DONE**.

**4. Record**

- **Card-speed bypass:** unplug and record.
- **Less noise reduction:** unplug, switch to Movie mode.

### Undo

| Mod | How to undo |
|---|---|
| Card-speed bypass | Pull the battery for a couple of seconds |
| Less noise reduction | Run `undo_less_noise_reduction.command`, or pull the battery |

Not sure if the noise-reduction mod is on?
Run `check_less_noise_reduction.command`. It only reads, it changes nothing.

## Open gate

Open gate records the full sensor at **3840 × 2560** (3:2) instead of 16:9.

It is **not part of this repo**. It is made by **ArcaneLantern**, and you get
it, and its instructions, from his project:

**[ArcaneLantern/zve1-open-gate-test](https://github.com/ArcaneLantern/zve1-open-gate-test)**

How his kit (v2) behaves on a ZV-E1 with firmware 2.02, as tested here:

| | |
|---|---|
| XAVC HS 4K at 24p | Works, records 3840 × 2560 |
| 60p | Crashes when recording starts |
| Live view | Sometimes turns purple after applying |
| Tracking frame | Looks stretched |
| Clips in an editor | Open as 16:9 until fixed, see the app below |
| After turning the camera off and on | Open gate is gone and has to be applied again |

Like the mods here, it only lives in the camera's memory and a battery pull
removes it. Questions and bug reports about open gate belong in his repo.

## Open Gate Fix app

Open gate clips are recorded at **3840 × 2560**, but the camera still labels
them 3840 × 2160, so editors like DaVinci Resolve open them as 16:9.
**Open Gate Fix** is a small app that corrects the label. Drop your clips on
the window and it saves fixed copies.

| System | Download |
|---|---|
| macOS (Apple silicon and Intel) | [**open-gate-fix-macos-v0.1.0.zip**](https://github.com/itsAllexB/sony-zv-e1-mods/releases/download/opengate-fix-v0.1.0/open-gate-fix-macos-v0.1.0.zip) |
| Windows (64-bit) | [**open-gate-fix-windows-v0.1.0.exe**](https://github.com/itsAllexB/sony-zv-e1-mods/releases/download/opengate-fix-v0.1.0/open-gate-fix-windows-v0.1.0.exe) |
| Linux (x86-64) | [**open-gate-fix-linux-v0.1.0.tar.gz**](https://github.com/itsAllexB/sony-zv-e1-mods/releases/download/opengate-fix-v0.1.0/open-gate-fix-linux-v0.1.0.tar.gz) |

- Nothing else to install. It is one file and works offline.
- Nothing is re-encoded. Only the size label changes, and the camera's gyro
  and lens data is kept.
- Your originals are left alone unless you turn on *Move originals to the
  trash* yourself.
- You choose where the fixed copies go: next to the originals, in a
  `remuxed` folder, or in one folder for everything.
- Normal 16:9 clips are recognised and skipped.

The app isn't signed with a paid developer certificate, so the first launch
may need an extra click: on macOS right-click the app and choose **Open**; on
Windows choose **More info → Run anyway** if SmartScreen appears.

How it works and how to build it: [`opengate-fix/`](opengate-fix/README.md).

## FAQ

<details>
<summary><b>Is this safe for my camera?</b></summary>
<br>
The mods change the camera's working memory only. Nothing is written to the
firmware, the menus or the saved settings, and a battery pull always puts
the camera back to stock. Before changing anything, each mod checks that it
is looking at a stock 2.02 camera. If anything doesn't match, it stops and
leaves the camera alone.
</details>

<details>
<summary><b>Does it work on other firmware or other cameras?</b></summary>
<br>
No. ZV-E1 on firmware 2.02 only. On anything else the mod refuses to run.
</details>

<details>
<summary><b>Why does it ask for my Mac password?</b></summary>
<br>
macOS needs admin rights to take the camera over from its own USB driver.
The password goes to macOS, not to the mod.
</details>

<details>
<summary><b>The card-speed mod is on but recording still stops.</b></summary>
<br>
The mod removes the camera's speed-class check. It can't make a card faster.
If the card really can't keep up with the bitrate, recording will still fail.
Use a faster card or a lower bitrate.
</details>

<details>
<summary><b>Do I have to re-run it every time?</b></summary>
<br>
Yes, after every full power-off or battery pull. The camera always starts
up stock.
</details>

<details>
<summary><b>Something went wrong. What now?</b></summary>
<br>
Pull the battery for a couple of seconds. That always resets everything. Then run
<code>check_readonly.command</code> to make sure the Mac can see the camera.
It reads only and changes nothing.
</details>

## Credits

- Made by **[itsAllexB](https://github.com/itsAllexB)**
- [**Sony-PMCA-RE**](https://github.com/ma1co/Sony-PMCA-RE) by ma1co (MIT),
  bundled for connecting to the camera from macOS
- Open Gate Fix uses [**egui**](https://github.com/emilk/egui), the
  [**Inter**](https://rsms.me/inter/) typeface (OFL) and
  [**Lucide**](https://lucide.dev) icons (ISC)
- The camera connection method comes from **ArcaneLantern's** open-gate kits

## License

[MIT](LICENSE). Use at your own risk.
