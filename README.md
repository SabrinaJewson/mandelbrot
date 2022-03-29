# GPU-based mandelbrot set renderer

Install with:

```sh
cargo install --git https://github.com/SabrinaJewson/mandelbrot
```

Run `mandelbrot` to open a window displaying the set, and `mandelbrot wallpaper` to set that window
as a wallpaper. Note that [keyboard controls do not work when in wallpaper
mode](https://github.com/rust-windowing/winit/issues/1997).

Controls:
- Click and drag: Move around
- Scroll wheel: Zoom in and out, holding control to zoom faster
- Up/down arrow keys or scroll wheel while right clicking: Change precision, holding control to
change faster
- Left click while right clicking: Change colour
- `r` or middle click: Reset to default position, zoom and precision
- `q` or escape: Exit

The set becomes blocky at high zoom levels because of the imprecision of 32-bit floats, the only
kind of floats supported by the GPU.
