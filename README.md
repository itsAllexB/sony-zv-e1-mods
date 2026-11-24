<div align="center">

# Sony ZV-E1 Mods

**Two small mods for the Sony ZV-E1, run from your Mac with a double-click.**

![Camera](https://img.shields.io/badge/camera-Sony%20ZV--E1-black)
![Firmware](https://img.shields.io/badge/firmware-2.02-blue)
![Platform](https://img.shields.io/badge/platform-macOS-lightgrey)
![License](https://img.shields.io/badge/license-MIT-green)

[Download](https://github.com/itsAllexB/sony-zv-e1-mods/releases) · [How to run](#how-to-run) · [Undo](#undo) · [FAQ](#faq)

</div>

---

> [!NOTE]
> Both mods only last until the camera powers off fully. Pull the battery for
> ten seconds and the camera is exactly back to stock. Nothing is saved to
> the camera's firmware or settings.

## The mods

| Mod | What it does | Undo |
|---|---|---|
| **Card-speed bypass** | Lets you record high-bitrate modes like **4K 120p** and **4K 60p 600 Mbps** on fast cards the camera doesn't recognise. | Battery pull |
| **Less noise reduction** | Turns off one of the camera's noise-reduction stages, so fine detail stops getting smoothed away between frames. Sharper picture, a bit more grain (roughly 15–25%). | Undo launcher or battery pull |

Each mod is its own download. Grab only the one you want.

> [!TIP]
> Looking for **open gate** (3840 × 2560, the full sensor)? That's a separate
> project by ArcaneLantern:
> **[ArcaneLantern/zve1-open-gate-test](https://github.com/ArcaneLantern/zve1-open-gate-test)**

## What you need

- A Sony **ZV-E1** on firmware **2.02** (check under *Setup → Version*)
- A Mac with Python 3.9 or newer
  (run `xcode-select --install` once if you don't have it)
- A USB-C **data** cable, plugged straight into the Mac
- A battery that's at least half charged

## How to run

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
- **Less noise reduction:** unplug, press the Still/Movie button
  **stills → movie → stills → movie**, then record.

## Undo

| Mod | How to undo |
|---|---|
| Card-speed bypass | Pull the battery for 10 seconds |
| Less noise reduction | Run `undo_less_noise_reduction.command`, or pull the battery |

Not sure if the noise-reduction mod is on?
Run `check_less_noise_reduction.command`. It only reads, it changes nothing.

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
Pull the battery for 10 seconds. That always resets everything. Then run
<code>check_readonly.command</code> to make sure the Mac can see the camera.
It reads only and changes nothing.
</details>

## Credits

- Made by **[itsAllexB](https://github.com/itsAllexB)**
- [**Sony-PMCA-RE**](https://github.com/ma1co/Sony-PMCA-RE) by ma1co (MIT),
  bundled for connecting to the camera from macOS
- The camera connection method comes from **ArcaneLantern's** open-gate kits

## License

[MIT](LICENSE). Use at your own risk.
