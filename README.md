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

If it's not clear, b -> brightness, c -> contrast, g -> gamma.

The default `classic` curve preserves the original formula and parameter defaults:

```text
y = clamp(C * x^(1/G) + (B - 1), 0, 1)
```

You can select the mid-grey-pivot `midgray` curve instead:

```bash
kde-gammactl --curve midgray -c 1.2 -b 0.1 -g 1.8
```

`midgray` defaults to contrast 1, brightness 0 and gamma 1; its supported
ranges are contrast 0.1–100, brightness -1–1 and gamma 0.3–4.4. `classic`
defaults to 1 for all three settings and requires only that gamma is positive.
The curve option is also available to the `set` and `generate` commands.

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

The classic curve uses this formula:

`y = clamp(contrast * x^(1/gamma) + (brightness - 1), 0, 1).`

The midgray curve pivots contrast at `127/255`, applies gamma afterward and
then adds brightness as an offset of `128/255` per brightness unit. Each run
uses the selected curve; omitting `--curve` always selects `classic`.

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
