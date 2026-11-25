# 1. Getting started

## Install

Download SketchCraft from the
[latest release](https://github.com/jasonsutter87/sketchcraft/releases/latest).
It's free.

| Your computer | Download | First time you open it |
| --- | --- | --- |
| **Mac** (Apple silicon or Intel) | the `.dmg`; drag SketchCraft to Applications | macOS asks if you trust it: **System Settings › Privacy & Security › Open Anyway** |
| **Windows 10 or 11** | the setup `.exe` or `.msi` | if Windows shows "Windows protected your PC": **More info › Run anyway** |
| **Windows, no installer** | the **portable ZIP**: unzip anywhere, open `SketchCraft.exe` | same as above |
| **Linux** (x64 or arm64) | `.AppImage` (any distro), `.deb` or `.rpm`; pick `amd64` / `x86_64` for most PCs, `aarch64` / `arm64` for ARM machines | nothing special |

Those first-time prompts appear because SketchCraft isn't signed by Apple or
Microsoft yet; they only ask once.

## Updates

From version 0.3.3, SketchCraft updates itself. A few seconds after it opens,
it checks for a new version and shows a card: **Update and restart**. Updates
are checked before they install. **File › Check for updates…** looks right
away, and the bottom of the File menu shows the version you have.

## Your first model

![Draw a rectangle, pull it up, type exact sizes](../demos/first-model.gif)

1. Press **R** (Rectangle), click one corner on the ground, then the other.
2. Press **P** (Push/Pull), click the face, move up, click.
3. Want exact sizes? Right after drawing, just type: `5,4` then Enter makes
   the rectangle 5 × 4 m; `3` then Enter makes the pull 3 m. The **Value**
   box in the corner shows what you type.
4. **⌘Z** (Ctrl+Z on Windows and Linux) undoes anything.

New models start with a **person** at the origin, for a sense of size. Pick a
different figure, or none, under **Document › Scale figure** in the right
panel.

## Finding your way around

| | |
| --- | --- |
| **Tool bar** | the drawing and editing tools. Tools that don't fit sit behind **+N more** (type to find one) |
| **View bar** | Iso, Top, Front, Right, Fit; X-Ray, Shadows, Edges, Parallel; Notes; Plan |
| **Axis gizmo** (top right of the model) | drag to orbit; click an axis to look along it (Z is the top view) |
| **Left panel** | Outliner (everything in the model), Library (doors, trees, cars, people…), Views (saved scenes) |
| **Right panel** | the model's settings, or whatever you've selected: its size, paint, finish |
| **Value box** | type exact sizes while a tool runs |
| **Shift+S** | search every command by name |
| **?** | every keyboard shortcut |
| **Learn** | short lessons that watch you do each step, and demos to watch |

**Getting around the model:** on a trackpad, two-finger swipe orbits,
Shift+swipe pans, pinch zooms. With a mouse, middle-drag orbits, right-drag
pans, the wheel zooms. **⇧Z** fits everything in view.

**A 3D mouse (3Dconnexion SpaceMouse)** orbits, pans and zooms too, in the desktop app on macOS, Windows and Linux.
Push or lift the cap to pan, push it away or pull it back to zoom, tilt it forward and back to orbit up and down, and
twist it to turn the model around. The left button fits everything in view; the right button goes to the iso view.
Plug it in and it works; nothing else needs setting up. **Window › 3D mouse…** has the settings: speed, how far the cap
must move first (the dead zone), "strongest movement only" so a straight push stays straight, a lock for rotation or
for pan, and a flip for each direction if the camera moves the wrong way for you. Without a 3D mouse nothing changes.

- It reads the device directly, so you do not need 3Dconnexion's driver (3DxWare). If the driver is installed it may keep
  the device to itself; if the camera does not move, quit the driver's background program and plug the mouse in again.
  (This has not been tried with every driver version: tell us what you see.)
- On Linux, run `spacenavd` (the app uses it when it is there) or give your user access to the device's `hidraw` file.
- It is the desktop app only: a browser tab does not read it.

Next: [Drawing](02-drawing.md)
