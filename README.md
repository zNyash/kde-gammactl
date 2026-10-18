# kde-gammactl

This basically lets you be able to control brightness, contrast and gamma on KDE Plasma Wayland with ICC profiles.
It's using the `kde_output_management_v2` KWin's protocol to set the ICC profile `vcgt` calibration automatically.

## How to install

Simply clone this repo and inside of it execute the `makepkg -si` command like this:

```bash
git clone https://github.com/zNyash/kde-gammactl.git
cd kde-gammactl
makepkg -si
```

## Usage

You can also open the app from your application menu or run:

```bash
kde-gammactl-gui
```

Choose a monitor or all monitors, select the curve, then adjust the sliders.
Changes are applied when you press Apply. Reset restores the original profile.

Pass the brightness, contrast and gamma values like this:

```bash
kde-gammactl -b 0.4 -c 0.8 -g 0.9
```

b -> brightness, c -> contrast and g -> gamma. The default curve is `classic`.
To use the Gamma Panel formula instead, add `--curve midgray`:

```bash
kde-gammactl --curve midgray -b 0.2 -c 1.2 -g 1.1
```

---

You can also set it up targeting a specific display:

```bash
kde-gammactl -o DP-1 -b 1 -c 1.1 -g 0.9
```

To get a list of your displays, type `kde-gammactl outputs`.

---

And to reset to the original settings just type

```bash
kde-gammactl reset
```

## Other

The classic formula is:

`y = clamp(contrast * x^(1/gamma) + (brightness - 1), 0, 1).`

The Gamma Panel formula is:

`y = clamp((contrast * (x - 127/255) + 127/255)^(1/gamma) + brightness * 128/255, 0, 1).`

Your data is on the `~/.local/share/kde-gammactl/`, the original ICC profile is saved the first time you make an adjustment so any other change you make always start from the original instead of stacking on top of the previous ones.

If a display doesn't have an ICC profile, kde-gammactl will use a neutral sRGB baseline instead.

If something goes wrong, these are the exit cores that can help you find out:

| Exit Code | Description                                         |
| --------: | --------------------------------------------------- |
|         1 | Failed to connect or communicate with the protocol. |
|         2 | KDE protocol not available.                         |
|         3 | Display not found or ambiguous.                     |
|         4 | Invalid ICC profile.                                |
|         5 | KWin rejected the profile.                          |
|         6 | Invalid arguments.                                  |
|         7 | Failed to access the data directory.                |
