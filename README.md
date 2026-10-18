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

You can just pass the arguments like this and this will set it up for all your displays:

```bash
kde-gammactl -b 0.4 -c 0.8 -g 0.9
```

If it's not clear, b -> brightness, c -> contrast, g -> gamma (where gamma needs to be > 0)

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

The brightness, contrast and gamma adjustments use this formula:

`y = clamp(contrast * x^(1/gamma) + (brightness - 1), 0, 1).`

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
